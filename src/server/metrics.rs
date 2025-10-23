/// Metrics HTTP server for Prometheus scraping
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::metrics;
use crate::server::shutdown::shutdown_receiver;

/// Metrics endpoint handler
async fn metrics_handler() -> Response {
    match metrics::get_metrics().gather() {
        Ok(output) => (StatusCode::OK, output).into_response(),
        Err(e) => {
            error!("Failed to gather metrics: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to gather metrics").into_response()
        }
    }
}

/// Start the metrics HTTP server
pub async fn start_metrics_server(bind_address: String, port: u16, shutdown_rx: broadcast::Receiver<()>) -> anyhow::Result<()> {
    let app = Router::new().route("/metrics", get(metrics_handler));

    let addr = format!("{}:{}", bind_address, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Metrics server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_receiver(shutdown_rx))
        .await
        .map_err(|e| {
            error!("Metrics server error: {}", e);
            anyhow::anyhow!("Metrics server failed: {}", e)
        })?;

    info!("Metrics server shut down gracefully");
    Ok(())
}
