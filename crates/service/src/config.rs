use std::{io, path::Path};

use http::Uri;
use serde::Deserialize;
use tokio::fs;

use crate::{
    account::Admin,
    crypto::KeyPair,
    endpoint::{enrollment::Issuer, Role},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config<T> {
    #[serde(with = "http_serde::uri")]
    pub host_address: Uri,
    pub description: String,
    pub admin: Admin,
    pub log_level: Option<String>,
    #[serde(flatten)]
    pub domain: T,
}

impl<T> Config<T>
where
    T: for<'de> Deserialize<'de>,
{
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let content = fs::read_to_string(path).await?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl<T> Config<T> {
    pub fn issuer(&self, role: Role, key_pair: KeyPair) -> Issuer {
        Issuer {
            key_pair,
            host_address: self.host_address.clone(),
            role,
            admin_name: self.admin.username.clone(),
            admin_email: self.admin.email.clone(),
            description: self.description.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("load config")]
    Load(#[from] io::Error),
    #[error("decode config")]
    Decode(#[from] toml::de::Error),
}
