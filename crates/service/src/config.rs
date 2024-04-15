//! Shared service configuration

use std::{io, path::Path};

use http::Uri;
use serde::Deserialize;
use tokio::fs;

use crate::{account::Admin, crypto::KeyPair, endpoint::enrollment::Issuer, tracing, Role};

/// Service configuration
#[derive(Debug, Clone, Deserialize)]
pub struct Config<T> {
    /// [`Uri`] this service is reachable from
    #[serde(with = "http_serde::uri")]
    pub host_address: Uri,
    /// Description of this service
    pub description: String,
    /// Admin details of this service
    pub admin: Admin,
    /// Tracing configuration
    #[serde(default)]
    pub tracing: tracing::Config,
    /// Generic configuartion that can be specialized by the service
    #[serde(flatten)]
    pub domain: T,
}

impl<T> Config<T>
where
    T: for<'de> Deserialize<'de>,
{
    /// Load configuration from the provided `path`
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let content = fs::read_to_string(path).await?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl<T> Config<T> {
    /// Construct [`Issuer`] details based on this [`Config`] and
    /// the provided [`Role`] and [`KeyPair`]
    pub fn issuer(&self, role: Role, key_pair: KeyPair) -> Issuer {
        Issuer {
            key_pair,
            host_address: self.host_address.clone(),
            role,
            admin_name: self.admin.name.clone(),
            admin_email: self.admin.email.clone(),
            description: self.description.clone(),
        }
    }
}

/// A config error
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Loading the config failed
    #[error("load config")]
    Load(#[from] io::Error),
    /// Decoding the config failed
    #[error("decode config")]
    Decode(#[from] toml::de::Error),
}
