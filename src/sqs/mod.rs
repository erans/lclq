//! AWS SQS dialect implementation.

pub mod handler;
pub mod request;
pub mod response;
pub mod server;
pub mod types;

pub use handler::*;
pub use request::*;
pub use response::*;
pub use server::*;
pub use types::*;
