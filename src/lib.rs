//! # lclq - Local Cloud Queue
//!
//! A lightweight local queue service compatible with AWS SQS and GCP Pub/Sub.
//!
//! lclq provides local implementations of AWS SQS and GCP Pub/Sub APIs, allowing you to
//! develop and test queue-based applications without cloud dependencies.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod core;
pub mod error;
pub mod metrics;
pub mod pubsub;
pub mod server;
pub mod sqs;
pub mod storage;
pub mod types;

pub use error::{Error, Result};
