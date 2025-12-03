//! Integration tests for push subscriptions.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use base64::Engine;
use lclq::pubsub::proto::*;
use lclq::pubsub::proto::publisher_server::Publisher;
use lclq::pubsub::proto::subscriber_server::Subscriber;
use lclq::storage::memory::InMemoryBackend;
use lclq::pubsub::push_queue::DeliveryQueue;
use lclq::pubsub::push_worker::PushWorkerPool;
use lclq::pubsub::publisher::PublisherService;
use lclq::pubsub::subscriber::SubscriberService;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Request;

#[derive(Clone)]
struct TestWebhookState {
    messages: Arc<Mutex<Vec<Value>>>,
}

async fn webhook_handler(
    State(state): State<TestWebhookState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let mut messages = state.messages.lock().await;
    messages.push(payload);
    StatusCode::OK
}

#[tokio::test]
async fn test_push_subscription_delivery() {
    // Start test webhook server
    let state = TestWebhookState {
        messages: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/webhook", post(webhook_handler))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let webhook_url = format!("http://{}/webhook", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create backend and services
    let backend = Arc::new(InMemoryBackend::new());
    let delivery_queue = DeliveryQueue::new();
    let worker_pool = PushWorkerPool::new(Some(2), delivery_queue.clone(), backend.clone());

    let publisher = PublisherService::new(backend.clone(), Some(delivery_queue));
    let subscriber = SubscriberService::new(backend.clone());

    // Create topic
    let topic_req = Request::new(Topic {
        name: "projects/test/topics/push-test".to_string(),
        ..Default::default()
    });
    publisher.create_topic(topic_req).await.unwrap();

    // Create push subscription
    let sub_req = Request::new(Subscription {
        name: "projects/test/subscriptions/push-sub".to_string(),
        topic: "projects/test/topics/push-test".to_string(),
        push_config: Some(PushConfig {
            push_endpoint: webhook_url.clone(),
            attributes: std::collections::HashMap::new(),
            authentication_method: None,
        }),
        retry_policy: Some(RetryPolicy {
            minimum_backoff: Some(prost_types::Duration {
                seconds: 1,
                nanos: 0,
            }),
            maximum_backoff: Some(prost_types::Duration {
                seconds: 5,
                nanos: 0,
            }),
        }),
        ack_deadline_seconds: 30,
        ..Default::default()
    });
    subscriber.create_subscription(sub_req).await.unwrap();

    // Publish message
    let pub_req = Request::new(PublishRequest {
        topic: "projects/test/topics/push-test".to_string(),
        messages: vec![PubsubMessage {
            data: b"Hello, Push!".to_vec(),
            attributes: [("key".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }],
    });
    publisher.publish(pub_req).await.unwrap();

    // Wait for delivery (with timeout)
    for _ in 0..50 {
        let messages = state.messages.lock().await;
        if !messages.is_empty() {
            break;
        }
        drop(messages);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Verify message was delivered
    let messages = state.messages.lock().await;
    assert_eq!(messages.len(), 1);

    let message = &messages[0];
    assert!(message["message"]["data"].is_string());
    assert_eq!(
        message["subscription"].as_str().unwrap(),
        "projects/test/subscriptions/push-sub"
    );

    // Decode base64 data
    let data_b64 = message["message"]["data"].as_str().unwrap();
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .unwrap();
    assert_eq!(std::str::from_utf8(&data).unwrap(), "Hello, Push!");

    // Shutdown worker pool
    worker_pool.shutdown().await;
}

#[tokio::test]
async fn test_push_subscription_retry_on_failure() {
    // Start test webhook server that fails first N times
    let state = Arc::new(Mutex::new(0u32));
    let state_clone = state.clone();

    let app = Router::new().route(
        "/webhook",
        post(move || {
            let state = state_clone.clone();
            async move {
                let mut count = state.lock().await;
                *count += 1;

                // Fail first 2 attempts, succeed on 3rd
                if *count < 3 {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::OK
                }
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let webhook_url = format!("http://{}/webhook", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create backend and services with fast retry
    let backend = Arc::new(InMemoryBackend::new());
    let delivery_queue = DeliveryQueue::new();
    let worker_pool = PushWorkerPool::new(Some(2), delivery_queue.clone(), backend.clone());

    let publisher = PublisherService::new(backend.clone(), Some(delivery_queue));
    let subscriber = SubscriberService::new(backend.clone());

    // Create topic and subscription with fast retry
    let topic_req = Request::new(Topic {
        name: "projects/test/topics/retry-test".to_string(),
        ..Default::default()
    });
    publisher.create_topic(topic_req).await.unwrap();

    let sub_req = Request::new(Subscription {
        name: "projects/test/subscriptions/retry-sub".to_string(),
        topic: "projects/test/topics/retry-test".to_string(),
        push_config: Some(PushConfig {
            push_endpoint: webhook_url,
            attributes: std::collections::HashMap::new(),
            authentication_method: None,
        }),
        retry_policy: Some(RetryPolicy {
            minimum_backoff: Some(prost_types::Duration {
                seconds: 1, // Fast retry for testing
                nanos: 0,
            }),
            maximum_backoff: Some(prost_types::Duration {
                seconds: 2,
                nanos: 0,
            }),
        }),
        ack_deadline_seconds: 30,
        ..Default::default()
    });
    subscriber.create_subscription(sub_req).await.unwrap();

    // Publish message
    let pub_req = Request::new(PublishRequest {
        topic: "projects/test/topics/retry-test".to_string(),
        messages: vec![PubsubMessage {
            data: b"Retry me!".to_vec(),
            ..Default::default()
        }],
    });
    publisher.publish(pub_req).await.unwrap();

    // Wait for retries and success
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify it was retried 3 times
    let count = state.lock().await;
    assert_eq!(*count, 3);

    worker_pool.shutdown().await;
}
