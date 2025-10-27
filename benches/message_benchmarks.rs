use chrono::Utc;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lclq::types::{Message, MessageAttributeValue, MessageId};
use std::collections::HashMap;

/// Create a test message with given body size
fn create_test_message(body_size: usize) -> Message {
    Message {
        id: MessageId::new(),
        body: "x".repeat(body_size),
        attributes: HashMap::new(),
        queue_id: "test-queue".to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    }
}

/// Create a message with attributes
fn create_message_with_attributes(body_size: usize, attr_count: usize) -> Message {
    let mut attributes = HashMap::new();
    for i in 0..attr_count {
        attributes.insert(
            format!("attr{}", i),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some(format!("value{}", i)),
                binary_value: None,
            },
        );
    }

    Message {
        id: MessageId::new(),
        body: "x".repeat(body_size),
        attributes,
        queue_id: "test-queue".to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    }
}

/// Benchmark MD5 hashing of message bodies with different sizes
fn bench_md5_body_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("md5_body_hash");

    for size in [100, 1024, 10240, 102400, 256_000].iter() {
        let body = "x".repeat(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                use md5::{Digest, Md5};
                let mut hasher = Md5::new();
                hasher.update(body.as_bytes());
                let result = hasher.finalize();
                black_box(format!("{:x}", result));
            });
        });
    }
    group.finish();
}

/// Benchmark message serialization to JSON
fn bench_message_serialize_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialize_json");

    for size in [100, 1024, 10240, 102400].iter() {
        let msg = create_test_message(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(serde_json::to_string(&msg).unwrap());
            });
        });
    }
    group.finish();
}

/// Benchmark message deserialization from JSON
fn bench_message_deserialize_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_deserialize_json");

    for size in [100, 1024, 10240, 102400].iter() {
        let msg = create_test_message(*size);
        let json = serde_json::to_string(&msg).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(serde_json::from_str::<Message>(&json).unwrap());
            });
        });
    }
    group.finish();
}

/// Benchmark message attribute serialization overhead
fn bench_message_with_attributes(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_with_attributes");

    for attr_count in [0, 1, 10, 100].iter() {
        let msg = create_message_with_attributes(1024, *attr_count);

        group.throughput(Throughput::Elements(*attr_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(attr_count),
            attr_count,
            |b, _| {
                b.iter(|| {
                    black_box(serde_json::to_string(&msg).unwrap());
                });
            },
        );
    }
    group.finish();
}

/// Benchmark UUID generation for message IDs
fn bench_uuid_generation(c: &mut Criterion) {
    c.bench_function("uuid_generation", |b| {
        b.iter(|| {
            black_box(uuid::Uuid::new_v4().to_string());
        });
    });
}

/// Benchmark message cloning (important for multi-consumer scenarios)
fn bench_message_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_clone");

    for size in [100, 1024, 10240, 102400].iter() {
        let msg = create_test_message(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(msg.clone());
            });
        });
    }
    group.finish();
}

/// Benchmark base64 encoding of receipt handles
fn bench_base64_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_encode");

    for size in [32, 64, 128, 256].iter() {
        let data = vec![0u8; *size];

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                use base64::{Engine as _, engine::general_purpose};
                black_box(general_purpose::STANDARD.encode(&data));
            });
        });
    }
    group.finish();
}

/// Benchmark base64 decoding of receipt handles
fn bench_base64_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_decode");

    for size in [32, 64, 128, 256].iter() {
        let data = vec![0u8; *size];
        use base64::{Engine as _, engine::general_purpose};
        let encoded = general_purpose::STANDARD.encode(&data);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(general_purpose::STANDARD.decode(&encoded).unwrap());
            });
        });
    }
    group.finish();
}

/// Benchmark HMAC-SHA256 signature generation (for receipt handles)
fn bench_hmac_signature(c: &mut Criterion) {
    let mut group = c.benchmark_group("hmac_signature");

    for size in [32, 64, 128, 256].iter() {
        let data = vec![0u8; *size];
        let key = b"test-secret-key-for-benchmarking-purposes";

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;

                let mut mac = HmacSha256::new_from_slice(key).unwrap();
                mac.update(&data);
                black_box(mac.finalize().into_bytes());
            });
        });
    }
    group.finish();
}

/// Benchmark HashMap operations (used for attributes)
fn bench_hashmap_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_operations");

    for count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &n| {
            b.iter(|| {
                let mut map = HashMap::new();
                for i in 0..n {
                    map.insert(format!("key{}", i), format!("value{}", i));
                }
                // Simulate lookups
                for i in 0..n {
                    black_box(map.get(&format!("key{}", i)));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_md5_body_hash,
    bench_message_serialize_json,
    bench_message_deserialize_json,
    bench_message_with_attributes,
    bench_uuid_generation,
    bench_message_clone,
    bench_base64_encode,
    bench_base64_decode,
    bench_hmac_signature,
    bench_hashmap_operations,
);
criterion_main!(benches);
