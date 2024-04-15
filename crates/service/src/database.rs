//! Shared service database

use std::path::Path;

use sqlx::{sqlite::SqliteConnectOptions, Pool, Sqlite, Transaction};
use thiserror::Error;

/// Service database
#[derive(Debug, Clone)]
pub struct Database {
    /// Connection pool to the underlying SQLITE database
    pub pool: Pool<Sqlite>,
}

impl Database {
    /// Opens a connection to the provided database path
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .read_only(false)
            .foreign_keys(true);

        Self::connect(options).await
    }

    async fn connect(options: SqliteConnectOptions) -> Result<Self, Error> {
        let pool = sqlx::SqlitePool::connect_with(options).await?;

        sqlx::migrate!("src/database/migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    /// Begin a database transaction
    pub async fn transaction(&self) -> Result<Transaction<'static, Sqlite>, Error> {
        Ok(self.pool.begin().await?)
    }
}

/// A database error
#[derive(Debug, Error)]
pub enum Error {
    /// Sqlx error
    #[error("sqlx")]
    Sqlx(#[from] sqlx::Error),
    /// Sqlx migration error
    #[error("sqlx migration")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
