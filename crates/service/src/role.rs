//! Defines the role a service plays in the infrastructure
use serde::{Deserialize, Serialize};

/// Service role
#[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Role {
    /// Hub
    Hub,
    /// Repository Manager
    RepositoryManager,
    /// Builder
    Builder,
}

impl Role {
    /// Service name associated to each role
    pub fn service_name(&self) -> &'static str {
        match self {
            Role::Hub => "summit",
            Role::RepositoryManager => "vessel",
            Role::Builder => "avalanche",
        }
    }
}
