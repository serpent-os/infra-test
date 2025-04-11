//! Defines the role a service plays in the infrastructure
use std::fmt;

use serde::{Deserialize, Serialize};

/// Service role
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumString, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
#[strum(serialize_all = "kebab-case")]
#[repr(u8)]
pub enum Role {
    /// Builder
    Builder = 1,
    /// Repository Manager
    RepositoryManager = 2,
    /// Hub
    Hub = 3,
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

impl From<Role> for u8 {
    fn from(role: Role) -> Self {
        role as u8
    }
}

impl TryFrom<u8> for Role {
    type Error = UnknownRole;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Role::Builder),
            2 => Ok(Role::RepositoryManager),
            3 => Ok(Role::Hub),
            x => Err(UnknownRole(x)),
        }
    }
}

/// Unknown [`Role`] from [`u8`]
#[derive(Debug)]
pub struct UnknownRole(u8);

impl fmt::Display for UnknownRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown role: {}", self.0)
    }
}

impl std::error::Error for UnknownRole {}
