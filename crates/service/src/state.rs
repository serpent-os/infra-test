//! Shared service state
use std::{
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;
use tokio::fs;
use tracing::debug;

use crate::{
    crypto::{self, KeyPair},
    database,
    endpoint::{self, enrollment},
    sync::SharedMap,
    Database,
};

/// Service state
#[derive(Debug, Clone)]
pub struct State {
    /// State directory
    pub dir: PathBuf,
    /// Database directory
    pub db_dir: PathBuf,
    /// Service database
    pub db: Database,
    /// Key pair used by the service
    pub key_pair: KeyPair,
    /// Pending enrollment requests that are awaiting confirmation
    ///
    /// Only applicable for hub service
    pub pending_sent: SharedMap<endpoint::Id, enrollment::Sent>,
}

impl State {
    /// Load state from the provided path. If no keypair and/or database exist, they will be created.
    #[tracing::instrument(name = "load_state", skip_all)]
    pub async fn load(root: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = root.as_ref().join("state");
        let db_dir = dir.join("db");

        if !db_dir.exists() {
            fs::create_dir_all(&db_dir).await.map_err(Error::CreateDbDir)?;
        }

        let db_path = db_dir.join("service.db");
        let key_path = dir.join(".privkey");

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
            dir,
            db_dir,
            db,
            key_pair,
            pending_sent: Default::default(),
        })
    }
}

/// A state error
#[derive(Debug, Error)]
pub enum Error {
    /// Error creating db directory
    #[error("create db directory")]
    CreateDbDir(#[source] io::Error),
    /// Loading database failed
    #[error("load database")]
    LoadDatabase(#[from] database::Error),
    /// Saving private key failed
    #[error("save private key")]
    SavePrivateKey(#[source] io::Error),
    /// Loading private key failed
    #[error("load private key")]
    LoadPrivateKey(#[source] io::Error),
    /// Decoding private key failed
    #[error("decode private key")]
    DecodePrivateKey(#[source] crypto::Error),
}
