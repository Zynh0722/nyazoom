use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};

use axum::http::StatusCode;
use axum::routing::post;
use axum::{
    extract::{DefaultBodyLimit, Multipart},
    response::Redirect,
    Router,
};

use futures::TryStreamExt;

use rand::distributions::{Alphanumeric, DistString};
use rand::rngs::SmallRng;
use rand::SeedableRng;

use sanitize_filename_reader_friendly::sanitize;

use std::io;
use std::net::SocketAddr;
use std::path::Path;

use tokio_util::compat::FuturesAsyncWriteCompatExt;
use tokio_util::io::StreamReader;

use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyazoom=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // uses create_dir_all to create both .cache and .temp inside it in one go
    make_dir(".cache/.temp").await?;
    make_dir(".cache/serve").await?;

    // Router Setup
    let with_big_body = Router::new()
        .route("/upload", post(upload_to_zip))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            10 * 1024 * 1024 * 1024, // 10GiB
        ));

    let base = Router::new()
        .nest_service("/", ServeDir::new("dist"))
        .nest_service("/download", ServeDir::new(".cache/serve"));

    let app = Router::new()
        .merge(with_big_body)
        .merge(base)
        .layer(TraceLayer::new_for_http());

    // Server creation
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on http://{}/", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn upload_to_zip(mut body: Multipart) -> Result<Redirect, (StatusCode, String)> {
    let cache_name = get_random_name(10);

    let archive_path = Path::new(".cache/serve").join(&format!("{}.zip", &cache_name));
    tracing::debug!("Zipping: {:?}", &archive_path);

    let mut archive = tokio::fs::File::create(archive_path)
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
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

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

    writer.close().await.unwrap();

    Ok(Redirect::to(&format!("/link.html?link={}.zip", cache_name)))
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

pub mod error {
    use std::io::{Error, ErrorKind};

    pub fn io_other(s: &str) -> Error {
        Error::new(ErrorKind::Other, s)
    }
}
