//! Shared service database

use std::path::Path;

use sqlx::{pool::PoolConnection, sqlite::SqliteConnectOptions, Pool, Sqlite, SqliteConnection};
use thiserror::Error;

/// Service database
#[derive(Debug, Clone)]
pub struct Database {
    /// Connection pool to the underlying SQLITE database
    pool: Pool<Sqlite>,
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
        let pool = sqlx::SqlitePool::connect_with(options).await.map_err(Error::Connect)?;

        sqlx::migrate!("src/database/migrations")
            .run(&pool)
            .await
            .map_err(Error::Migrate)?;

        Ok(Self { pool })
    }

    /// Acquire a database connection
    pub async fn acquire(&self) -> Result<PoolConnection<Sqlite>, Error> {
        self.pool.acquire().await.map_err(Error::Acquire)
    }

    /// Begin a database transaction
    pub async fn begin(&self) -> Result<Transaction, Error> {
        Ok(Transaction(self.pool.begin().await.map_err(Error::Commit)?))
    }
}

/// A database transaction
pub struct Transaction<'a>(sqlx::Transaction<'a, Sqlite>);

impl<'a> Transaction<'a> {
    /// Commit the transaction
    pub async fn commit(self) -> Result<(), Error> {
        self.0.commit().await.map_err(Error::Commit)
    }
}

impl<'a> AsMut<SqliteConnection> for Transaction<'a> {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        self.0.as_mut()
    }
}

/// Provides a database connection for executing queries
pub trait Executor<'a>: sqlx::Executor<'a, Database = Sqlite> {}

impl<'a, T> Executor<'a> for &'a mut T where &'a mut T: sqlx::Executor<'a, Database = Sqlite> {}

/// A database error
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to connect
    #[error("failed to connect")]
    Connect(#[source] sqlx::Error),
    /// Migrations failed
    #[error("migrations failed")]
    Migrate(#[source] sqlx::migrate::MigrateError),
    /// Acquire connection
    #[error("acquire connection")]
    Acquire(#[source] sqlx::Error),
    /// Begin transaction
    #[error("begin transaction")]
    Begin(#[source] sqlx::Error),
    /// Commit transaction
    #[error("commit transaction")]
    Commit(#[source] sqlx::Error),
    /// Execute query
    #[error("execute query")]
    Execute(#[source] sqlx::Error),
}
