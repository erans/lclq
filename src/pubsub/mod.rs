//! GCP Pub/Sub dialect implementation.
//!
//! This module implements Google Cloud Pub/Sub compatibility, including:
//! - Publisher service for topic management and message publishing
//! - Subscriber service for subscription management and message consumption
//! - Both gRPC and HTTP/REST protocols
//! - Message ordering with ordering keys
//! - Subscription filtering
//! - Push and pull subscriptions
//! - Dead letter topics

// Include generated protobuf code
/// Generated Protocol Buffer definitions for Google Cloud Pub/Sub v1 API.
///
/// This module contains the automatically generated Rust bindings for the
/// Google Cloud Pub/Sub gRPC API, including Publisher and Subscriber services.
#[allow(clippy::all, unused_imports, dead_code, missing_docs)]
pub mod proto {
    include!("generated/google.pubsub.v1.rs");
}

pub mod types;
pub mod publisher;
pub mod subscriber;
pub mod grpc_server;
pub mod rest;

pub use types::*;

