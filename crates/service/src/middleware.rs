//! Common gRPC middleware for handling logging and auth

pub use self::auth::{auth, Auth};
pub use self::log::{log_handler, Log};

pub mod auth;
pub mod log;
