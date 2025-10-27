/// Graceful shutdown handling for lclq servers
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::info;

/// Shutdown signal broadcaster
#[derive(Clone)]
pub struct ShutdownSignal {
    sender: broadcast::Sender<()>,
}

impl ShutdownSignal {
    /// Create a new shutdown signal
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1);
        Self { sender }
    }

    /// Subscribe to shutdown notifications
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }

    /// Trigger shutdown
    pub fn shutdown(&self) {
        let _ = self.sender.send(());
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
pub async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C), initiating graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        },
    }
}

/// Perform graceful shutdown with timeout
pub async fn shutdown_with_timeout(
    shutdown_signal: ShutdownSignal,
    timeout: Duration,
) -> anyhow::Result<()> {
    info!("Waiting for shutdown signal...");

    // Wait for shutdown signal
    wait_for_signal().await;

    // Broadcast shutdown to all servers
    info!("Broadcasting shutdown signal to all servers");
    shutdown_signal.shutdown();

    // Wait for graceful shutdown or timeout
    info!(
        "Waiting up to {:?} for servers to shutdown gracefully",
        timeout
    );

    tokio::time::sleep(timeout).await;

    info!("Shutdown complete");
    Ok(())
}

/// Create a shutdown receiver that completes when shutdown is triggered
pub async fn shutdown_receiver(mut rx: broadcast::Receiver<()>) {
    let _ = rx.recv().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_signal() {
        let signal = ShutdownSignal::new();
        let mut rx = signal.subscribe();

        // Trigger shutdown
        signal.shutdown();

        // Should receive signal
        assert!(rx.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let signal = ShutdownSignal::new();
        let mut rx1 = signal.subscribe();
        let mut rx2 = signal.subscribe();

        // Trigger shutdown
        signal.shutdown();

        // Both should receive signal
        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_receiver() {
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Spawn a task that waits for shutdown
        let handle = tokio::spawn(async move {
            shutdown_receiver(shutdown_rx).await;
        });

        // Give the receiver time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // The receiver task should complete
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(1), handle).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_signal_default() {
        let signal = ShutdownSignal::default();
        let mut rx = signal.subscribe();

        signal.shutdown();
        assert!(rx.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_receiver_immediate() {
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Send signal before receiver starts
        let _ = shutdown_tx.send(());

        // Receiver should complete immediately
        let handle = tokio::spawn(async move {
            shutdown_receiver(shutdown_rx).await;
        });

        let result = tokio::time::timeout(tokio::time::Duration::from_millis(100), handle).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_signal_clone() {
        let signal = ShutdownSignal::new();
        let cloned = signal.clone();

        let mut rx1 = signal.subscribe();
        let mut rx2 = cloned.subscribe();

        // Trigger shutdown from cloned signal
        cloned.shutdown();

        // Both should receive signal
        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
    }
}
