use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::routing::post;
use axum::BoxError;
use axum::{
    extract::{DefaultBodyLimit, Multipart},
    response::Redirect,
    Router,
};
use futures::{Stream, TryStreamExt};
use rand::distributions::{Alphanumeric, DistString};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use tokio::fs::File;
use tokio::io::BufWriter;
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

    // Router Setup
    let with_big_body = Router::new()
        .route("/upload", post(upload))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(
            10 * 1024 * 1024 * 1024, // 10GiB
        ));

    let base = Router::new()
        .nest_service("/", ServeDir::new("dist"))
        .nest_service("/download", ServeDir::new(".cache"));

    let app = Router::new()
        .merge(with_big_body)
        .merge(base)
        .layer(TraceLayer::new_for_http());

    // Server creation
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn upload(mut body: Multipart) -> Result<Redirect, (StatusCode, String)> {
    let cache_folder = Path::new(".cache/.temp").join(get_random_name(10));

    make_dir(&cache_folder)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    while let Some(field) = body.next_field().await.unwrap() {
        let file_name = if let Some(file_name) = field.file_name() {
            file_name.to_owned()
        } else {
            continue;
        };

        if !path_is_valid(&file_name) {
            return Err((StatusCode::BAD_REQUEST, "Invalid Filename >:(".to_owned()));
        }

        let path = cache_folder.join(file_name);

        tracing::debug!("\n\nstuff written to {path:?}\n");
        stream_to_file(&path, field).await?
    }

    remove_dir(cache_folder)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Redirect::to("/"))
}

async fn stream_to_file<S, E, P>(path: P, stream: S) -> Result<(), (StatusCode, String)>
where
    P: AsRef<Path>,
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        let mut file = BufWriter::new(File::create(&path).await?);

        // Copy the body into the file.
        tokio::io::copy(&mut body_reader, &mut file).await?;

        io::Result::Ok(())
    }
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

async fn remove_dir<T>(folder: T) -> io::Result<()>
where
    T: AsRef<Path>,
{
    tokio::fs::remove_dir_all(&folder).await?;

    Ok(())
}

#[inline]
fn path_is_valid(path: &str) -> bool {
    let mut components = Path::new(path).components().peekable();

    if let Some(first) = components.peek() {
        if !matches!(first, std::path::Component::Normal(_)) {
            return false;
        }
    }

    components.count() == 1
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
