//! Common middlewares used by built-in [`Server`]
//!
//! [`Server`]: crate::Server

pub use self::extract_token::ExtractToken;
pub use self::log::Log;

pub mod extract_token;
pub mod log;
