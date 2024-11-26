use service::database::{self, Transaction};
use sqlx::FromRow;
use thiserror::Error;

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

pub async fn lookup<'a, T>(conn: &'a mut T, name: &str) -> Result<Option<Record>, Error>
where
    &'a mut T: database::Executor<'a>,
{
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
    .fetch_optional(conn)
    .await?)
}

pub async fn list<'a, T>(conn: &'a mut T) -> Result<Vec<Record>, Error>
where
    &'a mut T: database::Executor<'a>,
{
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
    .fetch_all(conn)
    .await?)
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
