use derive_more::derive::{Display, From, Into};
use http::Uri;
use serde::{Deserialize, Serialize};
use service::database::Transaction;
use sqlx::FromRow;

use crate::project;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, From, Into, Display, FromRow)]
pub struct Id(i64);

#[derive(Debug, Clone)]
pub struct Repository {
    pub id: Id,
    pub name: String,
    pub summary: String,
    pub description: Option<String>,
    pub commit_ref: Option<String>,
    pub origin_uri: Uri,
    pub status: Status,
}

/// Status of the repository
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
    /// Never cloned before
    Fresh,
    /// Updating git ref
    Updating,
    /// Cloning for the first time
    Cloning,
    /// Indexing for updates
    Indexing,
    /// Doing nothing
    Idle,
}

pub async fn create(
    tx: &mut Transaction,
    project: project::Id,
    name: String,
    summary: String,
    origin_uri: Uri,
) -> Result<Repository, sqlx::Error> {
    let (id,): (i64,) = sqlx::query_as(
        "
        INSERT INTO repository
        (
          name,
          summary,
          origin_uri,
          status,
          project_id
        )
        VALUES (?,?,?,?,?)
        RETURNING repository_id;
        ",
    )
    .bind(&name)
    .bind(&summary)
    .bind(origin_uri.to_string())
    .bind(Status::Fresh.to_string())
    .bind(i64::from(project))
    .fetch_one(tx.as_mut())
    .await?;

    Ok(Repository {
        id: Id(id),
        name,
        summary,
        description: None,
        commit_ref: None,
        origin_uri,
        status: Status::Fresh,
    })
}
