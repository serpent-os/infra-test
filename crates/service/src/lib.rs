#![warn(missing_docs)]
//! Shared service code for Serpent OS infrastructure

pub use service_core::{auth, role};

pub use self::account::Account;
pub use self::client::Client;
pub use self::config::Config;
pub use self::database::Database;
pub use self::endpoint::Endpoint;
pub use self::role::Role;
pub use self::server::{start, Server};
pub use self::state::State;
pub use self::token::Token;

mod error;
mod middleware;
mod sync;

pub mod account;
pub mod api;
pub mod client;
pub mod config;
pub mod crypto;
pub mod database;
pub mod endpoint;
pub mod server;
pub mod signal;
pub mod state;
pub mod token;
pub mod tracing;
