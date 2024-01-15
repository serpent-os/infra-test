use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, strum::Display, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Role {
    Hub,
    RepositoryManager,
    Builder,
}

impl Role {
    pub fn service_name(&self) -> &'static str {
        match self {
            Role::Hub => "summit",
            Role::RepositoryManager => "vessel",
            Role::Builder => "avalanche",
        }
    }
}
