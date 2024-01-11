use std::path::Path;

use sqlx::{sqlite::SqliteConnectOptions, Pool, Sqlite, Transaction};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Database {
    pub pool: Pool<Sqlite>,
}

impl Database {
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

    pub async fn transaction(&self) -> Result<Transaction<'static, Sqlite>, Error> {
        Ok(self.pool.begin().await?)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlx migration")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
