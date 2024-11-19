use std::path::Path;

use sqlx::{prelude::FromRow, sqlite::SqliteConnectOptions, Pool, Sqlite};
use thiserror::Error;

pub type Transaction = sqlx::Transaction<'static, Sqlite>;

#[derive(Debug, Clone, FromRow)]
pub struct Record {
    pub name: String,
    pub source_id: String,
    pub package_id: String,
    pub build_release: i64,
    pub source_release: i64,
}

impl Record {
    pub fn new(id: moss::package::Id, meta: moss::package::Meta) -> Self {
        Self {
            name: meta.name.to_string(),
            source_id: meta.source_id,
            package_id: id.to_string(),
            build_release: meta.build_release as i64,
            source_release: meta.source_release as i64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
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

        sqlx::migrate!("src/collection_db/migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn begin(&self) -> Result<Transaction, Error> {
        Ok(self.pool.begin().await?)
    }

    pub async fn lookup(&self, name: &str) -> Result<Option<Record>, Error> {
        Ok(sqlx::query_as(
            "
            SELECT
              name,
              source_id,
              package_id,
              build_release,
              source_release
            FROM
              collection
            WHERE
              name = ?;
            ",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list(&self) -> Result<Vec<Record>, Error> {
        Ok(sqlx::query_as(
            "
            SELECT
              name,
              source_id,
              package_id,
              build_release,
              source_release
            FROM
              collection;
            ",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

pub async fn record(tx: &mut Transaction, record: Record) -> Result<(), Error> {
    sqlx::query(
        "
        INSERT INTO collection
        (
          name,
          source_id,
          package_id,
          build_release,
          source_release
        )
        VALUES (?,?,?,?,?)
        ON CONFLICT(name) DO UPDATE SET 
          source_id=excluded.source_id,
          package_id=excluded.package_id,
          build_release=excluded.build_release,
          source_release=excluded.source_release;
        ",
    )
    .bind(record.name)
    .bind(record.source_id)
    .bind(record.package_id)
    .bind(record.build_release)
    .bind(record.source_release)
    .execute(tx.as_mut())
    .await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlx migration")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
