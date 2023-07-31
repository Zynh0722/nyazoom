use async_zip::{tokio::write::ZipFileWriter, Compression, ZipEntryBuilder};

use axum::{
    body::StreamBody,
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, State},
    http::{Request, Response, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
    Json, Router, TypedHeader,
};

use futures::TryStreamExt;

use headers::HeaderMap;
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

use crate::state::AsyncRemoveRecord;
use crate::views::{DownloadLinkPage, HtmxPage, LinkView, Welcome};

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
    util::make_dir(".cache/serve").await?;

    let state = cache::fetch_cache().await;

    // Spawn a repeating task that will clean files periodically
    tokio::spawn({
        let state = state.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15 * 60)).await;
                tracing::info!("Cleaning Sweep!");

                let mut records = state.records.lock().await;

                for (key, record) in records.clone().into_iter() {
                    if !record.can_be_downloaded() {
                        tracing::info!("culling: {:?}", record);
                        records.remove_record(&key).await.unwrap();
                    }
                }
            }
        }
    });

    // Router Setup
    let app = Router::new()
        .route("/", get(welcome))
        .route("/upload", post(upload_to_zip))
        .route("/records", get(records))
        .route("/records/links", get(records_links))
        .route("/download/:id", get(download))
        .route("/link/:id", get(link))
        .route("/link/:id", delete(link_delete))
        .route("/link/:id/remaining", get(remaining))
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

async fn remaining(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let records = state.records.lock().await;
    if let Some(record) = records.get(&id) {
        let downloads_remaining = record.downloads_remaining();
        let plural = if downloads_remaining > 1 { "s" } else { "" };
        let out = format!(
            "You have {} download{} remaining!",
            downloads_remaining, plural
        );
        Html(out)
    } else {
        Html("?".to_string())
    }
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

// This function is to remain ugly until that time in which I properly hide
// this behind some kind of authentication
async fn records_links(State(state): State<AppState>) -> impl IntoResponse {
    let records = state.records.lock().await.clone();
    Html(leptos::ssr::render_to_string(move |cx| {
        leptos::view! { cx,
            <HtmxPage>
                <div class="form-wrapper">
                    <div class="column-container">
                        <ul>
                            {records.keys().map(|key| leptos::view! { cx,
                                        <li class="link-wrapper">
                                            <a href="/link/{key}">{key}</a>
                                            <button style="margin-left: 1em;"
                                                hx-target="closest .link-wrapper"
                                                hx-swap="outerHTML"
                                                hx-delete="/link/{key}">X</button>
                                        </li>
                                    })
                                .collect::<Vec<_>>()}
                        </ul>
                    </div>
                </div>
            </HtmxPage>
        }
    }))
}

async fn link(
    axum::extract::Path(id): axum::extract::Path<String>,
    State(mut state): State<AppState>,
) -> Result<Html<String>, Redirect> {
    {
        let mut records = state.records.lock().await;

        if let Some(record) = records.get_mut(&id) {
            if record.can_be_downloaded() {
                return Ok(Html(leptos::ssr::render_to_string({
                    let record = record.clone();
                    |cx| {
                        leptos::view! { cx, <DownloadLinkPage id=id record=record /> }
                    }
                })));
            }
        }
    }

    // TODO: This....
    state.remove_record(&id).await.unwrap();

    Err(Redirect::to(&format!("/404.html")))
}

async fn link_delete(
    axum::extract::Path(id): axum::extract::Path<String>,
    State(mut state): State<AppState>,
) -> Result<Html<String>, (StatusCode, String)> {
    state
        .remove_record(&id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Html("".to_string()))
}

async fn log_source<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    forwarded_for: Option<TypedHeader<ForwardedFor>>,
    req: Request<B>,
    next: Next<B>,
) -> impl IntoResponse {
    tracing::info!("{} : {:?}", addr, forwarded_for);

    next.run(req).await
}

async fn upload_to_zip(
    State(state): State<AppState>,
    mut body: Multipart,
) -> Result<Response<String>, (StatusCode, String)> {
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
    let record = UploadRecord::new(archive_path);
    records.insert(cache_name.clone(), record.clone());

    cache::write_to_cache(&records)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    writer.close().await.unwrap();

    let id = cache_name;
    let response = Response::builder()
        .status(200)
        .header("Content-Type", "text/html")
        .header("HX-Push-Url", format!("/link/{}", &id))
        .body(leptos::ssr::render_to_string(|cx| {
            leptos::view! { cx, <LinkView id record /> }
        }))
        .unwrap();

    Ok(response)
}

async fn download(
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
    State(mut state): State<AppState>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    {
        let mut records = state.records.lock().await;
        tracing::info!("{headers:?}");
        if headers.get("hx-request").is_some() {
            return Ok(axum::http::Response::builder()
                .header("HX-Redirect", format!("/download/{id}"))
                .status(204)
                .body("".to_owned())
                .unwrap()
                .into_response());
        }

        if let Some(record) = records.get_mut(&id) {
            if record.can_be_downloaded() {
                record.downloads += 1;

                let file = tokio::fs::File::open(&record.file).await.unwrap();

                return Ok(axum::response::Response::builder()
                    .header("Content-Type", "application/zip")
                    .body(StreamBody::new(ReaderStream::new(file)))
                    .unwrap()
                    .into_response());
            }
        }
    }

    // TODO: This....
    state.remove_record(&id).await.unwrap();

    Ok(Redirect::to("/404.html").into_response())
}
