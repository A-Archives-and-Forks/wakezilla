use anyhow::Result;
use axum::{
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

use crate::system;

pub async fn start(port: u16) -> Result<()> {
    start_with_shutdown(port, std::future::pending::<()>()).await
}

pub async fn start_with_shutdown(
    port: u16,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/machines/turn-off", post(turn_off_machine));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    let status = serde_json::json!({ "status": "ok" });
    Json(status)
}

async fn turn_off_machine() -> impl IntoResponse {
    system::shutdown_machine();
    (
        axum::http::StatusCode::OK,
        "Shutting down this machine".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn health_check_returns_ok_json() {
        let response = health_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
