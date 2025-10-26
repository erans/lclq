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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for oneshot

    #[tokio::test]
    async fn test_metrics_handler() {
        // Call the handler directly
        let response = metrics_handler().await;

        // Should return 200 OK with metrics data
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        // Create the app router
        let app = Router::new().route("/metrics", get(metrics_handler));

        // Create a request to /metrics
        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        // Send request to the app
        let response = app.oneshot(request).await.unwrap();

        // Verify response
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_start_metrics_server() {
        // Create shutdown signal
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Start server on a random port
        let server_handle = tokio::spawn(async move {
            start_metrics_server("127.0.0.1".to_string(), 0, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // Wait for server to shutdown
        let result = server_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_server_with_request() {
        use crate::server::shutdown::ShutdownSignal;

        let signal = ShutdownSignal::new();
        let shutdown_rx = signal.subscribe();

        // Start server on a specific port
        let port = 19091; // Use a high port to avoid conflicts
        let server_handle = tokio::spawn(async move {
            start_metrics_server("127.0.0.1".to_string(), port, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Make HTTP request to /metrics
        let url = format!("http://127.0.0.1:{}/metrics", port);
        let response = reqwest::get(&url).await;

        // Verify we can connect and get a response
        assert!(response.is_ok());
        if let Ok(resp) = response {
            assert_eq!(resp.status(), 200);
        }

        // Trigger shutdown
        signal.shutdown();

        // Wait for server to shutdown
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            server_handle
        ).await;

        assert!(result.is_ok());
    }
}
