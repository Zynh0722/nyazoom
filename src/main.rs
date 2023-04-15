use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};

use axum::body::StreamBody;
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::TypedHeader;
use axum::{
    extract::{DefaultBodyLimit, Multipart},
    response::Redirect,
    Router,
};

use futures::TryStreamExt;

use headers::{Header, HeaderName, HeaderValue};
use rand::distributions::{Alphanumeric, DistString};
use rand::rngs::SmallRng;
use rand::SeedableRng;

use sanitize_filename_reader_friendly::sanitize;

use serde::Serialize;

use tokio::io::AsyncReadExt;
use tokio_util::compat::FuturesAsyncWriteCompatExt;

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::path::Path;

use tokio_util::io::{ReaderStream, StreamReader};

use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod state;

use state::{AppState, UploadRecord};

pub mod error {
    use std::io::{Error, ErrorKind};

    pub fn io_other(s: &str) -> Error {
        Error::new(ErrorKind::Other, s)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Set up logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyazoom=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // uses create_dir_all to create both .cache and serve inside it in one go
    make_dir(".cache/serve").await?;

    let state = fetch_cache().await;

    // Router Setup
    let app = Router::new()
        .route("/upload", post(upload_to_zip))
        .route("/download/:id", get(download))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            10 * 1024 * 1024 * 1024, // 10GiB
        ))
        .with_state(state)
        .nest_service("/", ServeDir::new("dist"))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(log_source));

    // Server creation
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on http://{}/", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();

    Ok(())
}

// async fn log_source<B>(
//     ConnectInfo(addr): ConnectInfo<SocketAddr>,
//     req: Request<B>,
//     next: Next<B>,
// ) -> Response {
//     tracing::info!("{}", addr);
//
//     next.run(req).await
// }

async fn log_source<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    TypedHeader(ForwardedFor(forwarded_for)): TypedHeader<ForwardedFor>,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    tracing::info!("{} : {}", addr, forwarded_for);

    next.run(req).await
}

async fn upload_to_zip(
    State(state): State<AppState>,
    mut body: Multipart,
) -> Result<Redirect, (StatusCode, String)> {
    tracing::debug!("{:?}", *state.records.lock().await);

    let cache_name = get_random_name(10);

    let archive_path = Path::new(".cache/serve").join(&format!("{}.zip", &cache_name));

    tracing::debug!("Zipping: {:?}", &archive_path);

    let mut archive = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let mut writer = ZipFileWriter::new(&mut archive);

    while let Some(field) = body.next_field().await.unwrap() {
        let file_name = match field.file_name() {
            Some(file_name) => sanitize(file_name),
            _ => continue,
        };

        tracing::debug!("Downloading to Zip: {file_name:?}");

        let stream = field;
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let mut body_reader = StreamReader::new(body_with_io_error);

        let builder = ZipEntryBuilder::new(file_name, Compression::Deflate);
        let mut entry_writer = writer
            .write_entry_stream(builder)
            .await
            .unwrap()
            .compat_write();

        tokio::io::copy(&mut body_reader, &mut entry_writer)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        entry_writer
            .into_inner()
            .close()
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    }

    let mut records = state.records.lock().await;
    records.insert(cache_name.clone(), UploadRecord::new(archive_path));

    write_to_cache(&records)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    writer.close().await.unwrap();

    Ok(Redirect::to(&format!("/link.html?link={}", cache_name)))
}

async fn download(
    axum::extract::Path(id): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    let mut records = state.records.lock().await;

    if let Some(record) = records.get_mut(&id) {
        if record.can_be_downloaded() {
            record.downloads += 1;

            let file = tokio::fs::File::open(&record.file).await.unwrap();

            return Ok(axum::http::Response::builder()
                .header("Content-Type", "application/zip")
                .body(StreamBody::new(ReaderStream::new(file)))
                .unwrap()
                .into_response());
        } else {
            let _ = tokio::fs::remove_file(&record.file);
            records.remove(&id);
            write_to_cache(&records).await.unwrap();
        }
    }

    Ok(Redirect::to("/404.html").into_response())
}

#[inline]
async fn make_dir<T>(name: T) -> io::Result<()>
where
    T: AsRef<Path>,
{
    tokio::fs::create_dir_all(name)
        .await
        .or_else(|err| match err.kind() {
            io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(err),
        })
}

#[inline]
fn get_random_name(len: usize) -> String {
    let mut rng = SmallRng::from_entropy();

    Alphanumeric.sample_string(&mut rng, len)
}

async fn write_to_cache<T, Y>(records: &HashMap<T, Y>) -> io::Result<()>
where
    T: Serialize,
    Y: Serialize,
{
    let mut records_cache = tokio::fs::File::create(".cache/data").await.unwrap();

    let mut buf: Vec<u8> = Vec::with_capacity(200);
    bincode::serialize_into(&mut buf, &*records)
        .map_err(|err| error::io_other(&err.to_string()))?;

    let bytes_written = tokio::io::copy(&mut buf.as_slice(), &mut records_cache).await?;

    tracing::debug!("state cache size: {}", bytes_written);

    Ok(())
}

async fn fetch_cache() -> AppState {
    let records = if let Ok(file) = tokio::fs::File::open(".cache/data").await.as_mut() {
        let mut buf: Vec<u8> = Vec::with_capacity(200);
        file.read_to_end(&mut buf).await.unwrap();

        bincode::deserialize_from(&mut buf.as_slice()).unwrap()
    } else {
        HashMap::new()
    };

    AppState::new(records)
}

#[allow(dead_code)]
static UNITS: [&str; 6] = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
// This function is actually rather interesting to me, I understand that rust is
// very powerful, and its very safe, but i find it rather amusing that the [] operator
// doesn't check bounds, meaning it can panic at runtime. Usually rust is very
// very careful about possible panics
//
// although this function shouldn't be able to panic at runtime due to known bounds
// being listened to
#[inline]
fn _bytes_to_human_readable(bytes: u64) -> String {
    let mut running = bytes as f64;
    let mut count = 0;
    while running > 1024.0 && count <= 6 {
        running /= 1024.0;
        count += 1;
    }

    format!("{:.2} {}", running, UNITS[count - 1])
}

struct ForwardedFor(String);

static FF_TEXT: &str = "f-forwarded-for";
static FF_NAME: HeaderName = HeaderName::from_static(FF_TEXT);

impl Header for ForwardedFor {
    fn name() -> &'static HeaderName {
        &FF_NAME
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i headers::HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(headers::Error::invalid)?
            .to_str()
            .map_err(|_| headers::Error::invalid())?
            .to_owned();

        Ok(ForwardedFor(value))
    }

    fn encode<E: Extend<headers::HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(HeaderValue::from_str(&self.0).unwrap()));
    }
}
