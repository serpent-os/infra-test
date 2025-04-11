use service::database::{self, Transaction};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct Entry {
    pub name: String,
    pub source_id: String,
    pub package_id: String,
    pub build_release: i64,
    pub source_release: i64,
}

impl Entry {
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

pub async fn lookup<'a, T>(conn: &'a mut T, name: &str) -> sqlx::Result<Option<Entry>>
where
    &'a mut T: database::Executor<'a>,
{
    sqlx::query_as(
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
    .await
}

pub async fn list<'a, T>(conn: &'a mut T) -> sqlx::Result<Vec<Entry>>
where
    &'a mut T: database::Executor<'a>,
{
    sqlx::query_as(
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
    .await
}

pub async fn record(tx: &mut Transaction, entry: Entry) -> sqlx::Result<()> {
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
    .bind(entry.name)
    .bind(entry.source_id)
    .bind(entry.package_id)
    .bind(entry.build_release)
    .bind(entry.source_release)
    .execute(tx.as_mut())
    .await?;

    Ok(())
}
