use axum;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use hyper::header;
use std::env;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::RwLock;

pub async fn serve(rx: Arc<RwLock<Vec<u8>>>) {
    let app = Router::new()
        .route("/", get(render))
        .layer(Extension(Arc::new(rx)));

    let still_port = env::var("STILL_PORT")
        .unwrap_or(String::new())
        .parse::<u16>()
        .unwrap_or(80);

    let addr = SocketAddr::from(([0, 0, 0, 0], still_port));

    tokio::spawn(hyper::Server::bind(&addr).serve(app.into_make_service()));
}

async fn render(Extension(frame_buf): Extension<Arc<RwLock<Vec<u8>>>>) -> impl IntoResponse {
    let mut child = Command::new("ffmpeg")
        .args(&["-i", "-", "-vframes", "1", "-f", "image2", "pipe:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut headers = HeaderMap::new();
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = { frame_buf.read().await.clone() };
        if let Err(err) = stdin.write_all(&bytes).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                headers,
                Vec::from(format!("Failed to render image: {:?}", err)),
            );
        }
    }

    headers.insert(header::CONTENT_TYPE, "image/jpg".parse().unwrap());
    return (
        StatusCode::OK,
        headers,
        child.wait_with_output().await.unwrap().stdout,
    );
}
