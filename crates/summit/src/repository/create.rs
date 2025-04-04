use http::Uri;
use service::database::Transaction;

use crate::project;

use super::{Id, Repository, Status};

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
