use std::net::SocketAddr;

use axum::{
    extract::{DefaultBodyLimit, Multipart},
    response::{IntoResponse, Redirect},
    Router,
};
use axum::routing::post;
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyazoo=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let with_big_body = Router::new()
        .route("/upload", post(upload))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            250 * 1024 * 1024, // 250Mb
        ));

    let base = Router::new().nest_service("/", ServeDir::new("dist"));

    let app = Router::new().merge(with_big_body).merge(base);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.layer(TraceLayer::new_for_http()).into_make_service())
        .await
        .unwrap();
}

async fn upload(mut body: Multipart) -> impl IntoResponse {
    while let Some(field) = body.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let content_type = field.content_type().unwrap().to_string();
        let file_name = field.file_name().unwrap().to_string();
        let data = field.bytes().await.unwrap();

        tracing::debug!(
            "\n\nLength of {name} ({file_name}: {content_type}) is {} bytes\n",
            data.len()
        )
    }

    Redirect::to("/")
}
