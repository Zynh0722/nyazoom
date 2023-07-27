use async_zip::{tokio::write::ZipFileWriter, Compression, ZipEntryBuilder};

use axum::{
    body::StreamBody,
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router, TypedHeader,
};

use futures::TryStreamExt;

use leptos::IntoView;
use nyazoom_headers::ForwardedFor;

use sanitize_filename_reader_friendly::sanitize;

use std::{io, net::SocketAddr, path::Path, time::Duration};

use tokio_util::{
    compat::FuturesAsyncWriteCompatExt,
    io::{ReaderStream, StreamReader},
};

use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cache;
mod nyazoom_headers;
mod state;
mod util;
mod views;

use state::{AppState, UploadRecord};

use crate::views::{DownloadLink, Welcome};

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

    // Spawn a repeating task that will clean files periodically
    tokio::spawn(async {
        loop {
            tracing::info!("Cleaning Sweep!");
            tokio::time::sleep(Duration::from_secs(15 * 60)).await
        }
    });

    // uses create_dir_all to create both .cache and serve inside it in one go
    util::make_dir(".cache/serve").await?;

    let state = cache::fetch_cache().await;

    // Router Setup
    let app = Router::new()
        .route("/", get(welcome))
        .route("/upload", post(upload_to_zip))
        .route("/records", get(records))
        .route("/records/links", get(records_links))
        .route("/download/:id", get(download))
        .route("/link/:id", get(link))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            10 * 1024 * 1024 * 1024, // 10GiB
        ))
        .with_state(state)
        .fallback_service(ServeDir::new("dist"))
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

async fn welcome() -> impl IntoResponse {
    let cat_fact = views::get_cat_fact().await;
    Html(leptos::ssr::render_to_string(move |cx| {
        leptos::view! { cx, <Welcome fact=cat_fact /> }
    }))
}

async fn records(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.records.lock().await.clone())
}

async fn records_links(State(state): State<AppState>) -> impl IntoResponse {
    let records = state.records.lock().await.clone();
    Html(leptos::ssr::render_to_string(move |cx| {
        leptos::view! { cx,
            <ul>
                {records
                    .iter()
                    .map(|(key, _)|
                        leptos::view! { cx, <li><a href="/link/{key}">{key}</a></li> })
                    .collect::<Vec<_>>()}
            </ul>
        }
    }))
}

async fn link(
    axum::extract::Path(id): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, Redirect> {
    let mut records = state.records.lock().await;

    if let Some(record) = records.get_mut(&id) {
        if record.can_be_downloaded() {
            return Ok(Html(leptos::ssr::render_to_string({
                let record = record.clone();
                |cx| {
                    leptos::view! { cx, <DownloadLink id=id record=record /> }
                }
            })));
        } else {
            let _ = tokio::fs::remove_file(&record.file).await;
            records.remove(&id);
            cache::write_to_cache(&records).await.unwrap();
        }
    }

    Err(Redirect::to(&format!("/404.html")))
}

async fn log_source<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    forwarded_for: Option<TypedHeader<ForwardedFor>>,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    tracing::info!("{} : {:?}", addr, forwarded_for);

    next.run(req).await
}

async fn upload_to_zip(
    State(state): State<AppState>,
    mut body: Multipart,
) -> Result<Redirect, (StatusCode, String)> {
    tracing::debug!("{:?}", *state.records.lock().await);

    let cache_name = util::get_random_name(10);

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

    cache::write_to_cache(&records)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    writer.close().await.unwrap();

    Ok(Redirect::to(&format!("/link/{}", cache_name)))
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
            let _ = tokio::fs::remove_file(&record.file).await;
            records.remove(&id);
            cache::write_to_cache(&records).await.unwrap();
        }
    }

    Ok(Redirect::to("/404.html").into_response())
}
