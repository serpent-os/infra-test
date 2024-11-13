//! API types
pub use self::operation::Operation;

pub mod operation;

/// API version
#[derive(Debug, Clone, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum Version {
    /// Version 1
    V1,
}