use std::{io, path::Path};

use thiserror::Error;
use tokio::fs;
use tracing::debug;

use crate::{
    crypto::{self, KeyPair},
    database,
    endpoint::enrollment,
    Database,
};

#[derive(Debug, Clone)]
pub struct State {
    pub db: Database,
    pub key_pair: KeyPair,
    pub pending_sent_enrollment: enrollment::PendingSent,
    pub pending_received_enrollment: enrollment::PendingReceived,
}

impl State {
    #[tracing::instrument(name = "load_state", skip_all)]
    pub async fn load(root: impl AsRef<Path>) -> Result<Self, Error> {
        let db_path = root.as_ref().join("service.db");
        let key_path = root.as_ref().join(".privkey");

        let db = Database::new(&db_path).await?;
        debug!(path = ?db_path, "Database opened");

        let key_pair = if !key_path.exists() {
            let key_pair = KeyPair::generate();
            debug!(key_pair = %key_pair.public_key(), "Keypair generated");

            fs::write(&key_path, &key_pair.to_bytes())
                .await
                .map_err(Error::SavePrivateKey)?;

            key_pair
        } else {
            let bytes = fs::read(&key_path).await.map_err(Error::LoadPrivateKey)?;

            let key_pair = KeyPair::try_from_bytes(&bytes).map_err(Error::DecodePrivateKey)?;
            debug!(key_pair = %key_pair.public_key(), "Keypair loaded");

            key_pair
        };

        Ok(Self {
            db,
            key_pair,
            pending_sent_enrollment: Default::default(),
            pending_received_enrollment: Default::default(),
        })
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("load database")]
    LoadDatabase(#[from] database::Error),
    #[error("save private key")]
    SavePrivateKey(#[source] io::Error),
    #[error("load private key")]
    LoadPrivateKey(#[source] io::Error),
    #[error("decode private key")]
    DecodePrivateKey(#[source] crypto::Error),
}
