use chrono::Utc;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lclq::storage::StorageBackend;
use lclq::storage::memory::InMemoryBackend;
use lclq::types::{Message, MessageId, QueueConfig, QueueType, ReceiveOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Create a test queue
async fn create_test_queue(backend: &Arc<dyn StorageBackend>) -> QueueConfig {
    let config = QueueConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("bench-queue-{}", uuid::Uuid::new_v4()),
        queue_type: QueueType::SqsStandard,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };
    backend.create_queue(config).await.unwrap()
}

/// Create a test message
fn create_test_message(queue_id: &str, body: String) -> Message {
    Message {
        id: MessageId::new(),
        body,
        attributes: HashMap::new(),
        queue_id: queue_id.to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    }
}

/// Benchmark send_message with different message sizes
fn bench_send_message(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("send_message");

    for size in [100, 1024, 10240, 102400].iter() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let queue = rt.block_on(create_test_queue(&backend));
        let body = "x".repeat(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.to_async(&rt).iter(|| async {
                let msg = create_test_message(&queue.id, body.clone());
                black_box(backend.send_message(&queue.id, msg).await.unwrap());
            });
        });
    }
    group.finish();
}

/// Benchmark send_messages (batch send) with different batch sizes
fn bench_send_messages_batch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("send_messages_batch");

    for batch_size in [1, 10, 100, 1000].iter() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let queue = rt.block_on(create_test_queue(&backend));
        let body = "test message body".to_string();

        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let messages: Vec<Message> = (0..size)
                        .map(|_| create_test_message(&queue.id, body.clone()))
                        .collect();
                    black_box(backend.send_messages(&queue.id, messages).await.unwrap());
                });
            },
        );
    }
    group.finish();
}

/// Benchmark receive_messages with different max_messages
fn bench_receive_messages(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("receive_messages");

    for max_messages in [1, 10, 100].iter() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let queue = rt.block_on(create_test_queue(&backend));

        // Pre-populate queue with messages
        let messages: Vec<Message> = (0..1000)
            .map(|_| create_test_message(&queue.id, "test".to_string()))
            .collect();
        rt.block_on(backend.send_messages(&queue.id, messages))
            .unwrap();

        group.throughput(Throughput::Elements(*max_messages as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(max_messages),
            max_messages,
            |b, &max| {
                b.to_async(&rt).iter(|| async {
                    let options = ReceiveOptions {
                        max_messages: max,
                        visibility_timeout: Some(30),
                        wait_time_seconds: 0,
                        attribute_names: vec![],
                        message_attribute_names: vec![],
                    };
                    black_box(backend.receive_messages(&queue.id, options).await.unwrap());
                });
            },
        );
    }
    group.finish();
}

/// Benchmark delete_message
fn bench_delete_message(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
    let queue = rt.block_on(create_test_queue(&backend));

    c.bench_function("delete_message", |b| {
        b.to_async(&rt).iter(|| async {
            // Send a message
            let msg = create_test_message(&queue.id, "test".to_string());
            backend.send_message(&queue.id, msg).await.unwrap();

            // Receive it to get receipt handle
            let options = ReceiveOptions {
                max_messages: 1,
                visibility_timeout: Some(30),
                wait_time_seconds: 0,
                attribute_names: vec![],
                message_attribute_names: vec![],
            };
            let received = backend.receive_messages(&queue.id, options).await.unwrap();
            let receipt_handle = received[0].receipt_handle.clone();

            // Delete it
            backend
                .delete_message(&queue.id, &receipt_handle)
                .await
                .unwrap();
        });
    });
}

/// Benchmark full round-trip: send + receive + delete
fn bench_round_trip(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
    let queue = rt.block_on(create_test_queue(&backend));

    c.bench_function("round_trip_send_receive_delete", |b| {
        b.to_async(&rt).iter(|| async {
            // Send
            let msg = create_test_message(&queue.id, "test message".to_string());
            backend.send_message(&queue.id, msg).await.unwrap();

            // Receive
            let options = ReceiveOptions {
                max_messages: 1,
                visibility_timeout: Some(30),
                wait_time_seconds: 0,
                attribute_names: vec![],
                message_attribute_names: vec![],
            };
            let received = backend.receive_messages(&queue.id, options).await.unwrap();
            let receipt_handle = received[0].receipt_handle.clone();

            // Delete
            backend
                .delete_message(&queue.id, &receipt_handle)
                .await
                .unwrap();
        });
    });
}

/// Benchmark purge_queue
fn bench_purge_queue(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("purge_queue");

    for msg_count in [100, 1000, 10000].iter() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let queue = rt.block_on(create_test_queue(&backend));

        group.throughput(Throughput::Elements(*msg_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(msg_count),
            msg_count,
            |b, &count| {
                b.iter_batched(
                    || {
                        // Setup: populate queue with messages
                        let messages: Vec<Message> = (0..count)
                            .map(|_| create_test_message(&queue.id, "test".to_string()))
                            .collect();
                        rt.block_on(backend.send_messages(&queue.id, messages))
                            .unwrap();
                    },
                    |_| {
                        // Benchmark: purge the queue
                        rt.block_on(async {
                            backend.purge_queue(&queue.id).await.unwrap();
                        });
                    },
                    criterion::BatchSize::PerIteration,
                );
            },
        );
    }
    group.finish();
}

/// Benchmark get_stats
fn bench_get_stats(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
    let queue = rt.block_on(create_test_queue(&backend));

    // Populate with some messages
    let messages: Vec<Message> = (0..1000)
        .map(|_| create_test_message(&queue.id, "test".to_string()))
        .collect();
    rt.block_on(backend.send_messages(&queue.id, messages))
        .unwrap();

    c.bench_function("get_stats", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(backend.get_stats(&queue.id).await.unwrap());
        });
    });
}

/// Benchmark concurrent send operations
fn bench_concurrent_sends(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_sends");

    for concurrency in [1, 10, 100].iter() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let queue = rt.block_on(create_test_queue(&backend));

        group.throughput(Throughput::Elements(*concurrency as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &conc| {
                b.to_async(&rt).iter(|| async {
                    let handles: Vec<_> = (0..conc)
                        .map(|_| {
                            let backend = backend.clone();
                            let queue_id = queue.id.clone();
                            tokio::spawn(async move {
                                let msg = create_test_message(&queue_id, "test".to_string());
                                backend.send_message(&queue_id, msg).await.unwrap();
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_send_message,
    bench_send_messages_batch,
    bench_receive_messages,
    bench_delete_message,
    bench_round_trip,
    bench_purge_queue,
    bench_get_stats,
    bench_concurrent_sends,
);
criterion_main!(benches);
