//! Primitive service types

// #![warn(missing_docs)]

pub use self::collectable::Collectable;
pub use self::remote::Remote;
pub use self::role::Role;

pub mod api;
pub mod auth;
pub mod collectable;
pub mod endpoint;
pub mod remote;
pub mod role;
