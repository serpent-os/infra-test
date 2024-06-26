#![warn(missing_docs)]
//! Shared service code for Serpent OS infrastructure

pub use self::account::Account;
pub use self::config::Config;
pub use self::database::Database;
pub use self::endpoint::Endpoint;
pub use self::role::Role;
pub use self::server::{start, Server};
pub use self::state::State;
pub use self::token::Token;

pub mod account;
pub mod config;
pub mod crypto;
pub mod database;
pub mod endpoint;
mod error;
pub mod middleware;
pub mod role;
pub mod server;
pub mod signal;
pub mod state;
pub mod sync;
pub mod token;
pub mod tracing;
