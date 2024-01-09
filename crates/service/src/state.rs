use std::{io, path::Path};

use log::debug;
use thiserror::Error;
use tokio::fs;

use crate::{
    crypto::{self, KeyPair},
    database,
    endpoint::enrollment::PendingEnrollment,
    Database,
};

#[derive(Debug, Clone)]
pub struct State {
    pub db: Database,
    pub key_pair: KeyPair,
    pub pending_enrollment: PendingEnrollment,
}

impl State {
    pub async fn load(root: impl AsRef<Path>) -> Result<Self, Error> {
        let db_path = root.as_ref().join("service.db");
        let key_path = root.as_ref().join(".privkey");

        let db = Database::new(&db_path).await?;
        debug!("database {db_path:?} opened");

        let key_pair = if !key_path.exists() {
            let key_pair = KeyPair::generate();
            debug!("keypair generated: {}", key_pair.public_key().encode());

            fs::write(&key_path, &key_pair.to_bytes())
                .await
                .map_err(Error::SavePrivateKey)?;

            key_pair
        } else {
            let bytes = fs::read(&key_path).await.map_err(Error::LoadPrivateKey)?;

            let key_pair = KeyPair::try_from_bytes(&bytes).map_err(Error::DecodePrivateKey)?;
            debug!("keypair loaded: {}", key_pair.public_key().encode());

            key_pair
        };

        Ok(Self {
            db,
            key_pair,
            pending_enrollment: PendingEnrollment::default(),
        })
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("load database: {0}")]
    LoadDatabase(#[from] database::Error),
    #[error("save private key: {0}")]
    SavePrivateKey(#[source] io::Error),
    #[error("load private key: {0}")]
    LoadPrivateKey(#[source] io::Error),
    #[error("decode private key: {0}")]
    DecodePrivateKey(#[source] crypto::Error),
}
