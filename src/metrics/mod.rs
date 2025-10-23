///! Metrics and monitoring using Prometheus.

use once_cell::sync::Lazy;
use prometheus::{
    CounterVec, Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry, TextEncoder,
};
use std::sync::Arc;
use tracing::error;

/// Global metrics registry
static METRICS_REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

/// Global metrics instance
pub static METRICS: Lazy<Arc<Metrics>> = Lazy::new(|| {
    let metrics = Metrics::new();
    if let Err(e) = metrics.register(&METRICS_REGISTRY) {
        error!("Failed to register metrics: {}", e);
    }
    Arc::new(metrics)
});

/// Metrics collector for lclq
pub struct Metrics {
    // Counter metrics
    pub messages_sent_total: IntCounterVec,
    pub messages_received_total: IntCounterVec,
    pub messages_deleted_total: IntCounterVec,
    pub messages_to_dlq_total: IntCounterVec,
    pub backend_errors_total: IntCounterVec,
    pub api_requests_total: IntCounterVec,

    // Histogram metrics (for latencies)
    pub send_latency_seconds: HistogramVec,
    pub receive_latency_seconds: HistogramVec,
    pub api_latency_seconds: HistogramVec,

    // Gauge metrics
    pub queue_depth: IntGaugeVec,
    pub in_flight_messages: IntGaugeVec,
    pub queue_count: IntGauge,
    pub active_connections: IntGaugeVec,
}

impl Metrics {
    /// Create a new Metrics instance
    pub fn new() -> Self {
        // Counter metrics
        let messages_sent_total = IntCounterVec::new(
            Opts::new("lclq_messages_sent_total", "Total messages sent to queues"),
            &["queue_id", "queue_name", "dialect"],
        )
        .expect("Failed to create messages_sent_total metric");

        let messages_received_total = IntCounterVec::new(
            Opts::new(
                "lclq_messages_received_total",
                "Total messages received from queues",
            ),
            &["queue_id", "queue_name", "dialect"],
        )
        .expect("Failed to create messages_received_total metric");

        let messages_deleted_total = IntCounterVec::new(
            Opts::new(
                "lclq_messages_deleted_total",
                "Total messages deleted from queues",
            ),
            &["queue_id", "queue_name"],
        )
        .expect("Failed to create messages_deleted_total metric");

        let messages_to_dlq_total = IntCounterVec::new(
            Opts::new(
                "lclq_messages_to_dlq_total",
                "Total messages moved to dead letter queue",
            ),
            &["queue_id", "queue_name"],
        )
        .expect("Failed to create messages_to_dlq_total metric");

        let backend_errors_total = IntCounterVec::new(
            Opts::new("lclq_backend_errors_total", "Total backend errors"),
            &["backend", "operation"],
        )
        .expect("Failed to create backend_errors_total metric");

        let api_requests_total = IntCounterVec::new(
            Opts::new("lclq_api_requests_total", "Total API requests"),
            &["api", "method", "endpoint", "status"],
        )
        .expect("Failed to create api_requests_total metric");

        // Histogram metrics
        let send_latency_seconds = HistogramVec::new(
            HistogramOpts::new("lclq_send_latency_seconds", "Send message latency in seconds")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["backend"],
        )
        .expect("Failed to create send_latency_seconds metric");

        let receive_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "lclq_receive_latency_seconds",
                "Receive message latency in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["backend"],
        )
        .expect("Failed to create receive_latency_seconds metric");

        let api_latency_seconds = HistogramVec::new(
            HistogramOpts::new("lclq_api_latency_seconds", "API request latency in seconds")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["api", "endpoint"],
        )
        .expect("Failed to create api_latency_seconds metric");

        // Gauge metrics
        let queue_depth = IntGaugeVec::new(
            Opts::new("lclq_queue_depth", "Current number of available messages in queue"),
            &["queue_id", "queue_name"],
        )
        .expect("Failed to create queue_depth metric");

        let in_flight_messages = IntGaugeVec::new(
            Opts::new(
                "lclq_in_flight_messages",
                "Current number of in-flight messages",
            ),
            &["queue_id", "queue_name"],
        )
        .expect("Failed to create in_flight_messages metric");

        let queue_count = IntGauge::new("lclq_queue_count", "Total number of queues")
            .expect("Failed to create queue_count metric");

        let active_connections = IntGaugeVec::new(
            Opts::new("lclq_active_connections", "Number of active connections"),
            &["dialect"],
        )
        .expect("Failed to create active_connections metric");

        Self {
            messages_sent_total,
            messages_received_total,
            messages_deleted_total,
            messages_to_dlq_total,
            backend_errors_total,
            api_requests_total,
            send_latency_seconds,
            receive_latency_seconds,
            api_latency_seconds,
            queue_depth,
            in_flight_messages,
            queue_count,
            active_connections,
        }
    }

    /// Register all metrics with the registry
    fn register(&self, registry: &Registry) -> Result<(), prometheus::Error> {
        registry.register(Box::new(self.messages_sent_total.clone()))?;
        registry.register(Box::new(self.messages_received_total.clone()))?;
        registry.register(Box::new(self.messages_deleted_total.clone()))?;
        registry.register(Box::new(self.messages_to_dlq_total.clone()))?;
        registry.register(Box::new(self.backend_errors_total.clone()))?;
        registry.register(Box::new(self.api_requests_total.clone()))?;
        registry.register(Box::new(self.send_latency_seconds.clone()))?;
        registry.register(Box::new(self.receive_latency_seconds.clone()))?;
        registry.register(Box::new(self.api_latency_seconds.clone()))?;
        registry.register(Box::new(self.queue_depth.clone()))?;
        registry.register(Box::new(self.in_flight_messages.clone()))?;
        registry.register(Box::new(self.queue_count.clone()))?;
        registry.register(Box::new(self.active_connections.clone()))?;
        Ok(())
    }

    /// Gather metrics in Prometheus text format
    pub fn gather(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = METRICS_REGISTRY.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer).unwrap_or_default())
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the global metrics instance
pub fn get_metrics() -> Arc<Metrics> {
    METRICS.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        // Test that metrics can be created without panicking
        metrics
            .messages_sent_total
            .with_label_values(&["test-id", "test-queue", "sqs"])
            .inc();
    }

    #[test]
    fn test_metrics_gather() {
        let metrics = get_metrics();
        let output = metrics.gather().expect("Failed to gather metrics");
        assert!(output.contains("lclq_"));
    }
}
