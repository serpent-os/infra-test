use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use itertools::Itertools;
use moss::package;
use sqlx::{SqliteConnection, prelude::FromRow};

use crate::{profile, project, repository};

use super::{Id, Status, Task};

#[derive(Debug, Default)]
pub struct Params {
    id: Option<Id>,
    statuses: Option<Vec<Status>>,
}

impl Params {
    pub fn id(self, id: Id) -> Self {
        Self { id: Some(id), ..self }
    }

    pub fn statuses(self, statuses: impl IntoIterator<Item = Status>) -> Self {
        Self {
            statuses: Some(statuses.into_iter().collect()),
            ..self
        }
    }
}

pub async fn query(conn: &mut SqliteConnection, params: Params) -> Result<Vec<Task>> {
    #[derive(FromRow)]
    struct Row {
        #[sqlx(rename = "task_id", try_from = "i64")]
        id: Id,
        #[sqlx(try_from = "i64")]
        project_id: project::Id,
        #[sqlx(try_from = "i64")]
        profile_id: profile::Id,
        #[sqlx(try_from = "i64")]
        repository_id: repository::Id,
        slug: String,
        #[sqlx(try_from = "String")]
        package_id: package::Id,
        arch: String,
        build_id: String,
        description: String,
        commit_ref: String,
        source_path: String,
        #[sqlx(try_from = "&'a str")]
        status: Status,
        allocated_builder: Option<String>,
        log_path: Option<String>,
        started: DateTime<Utc>,
        updated: DateTime<Utc>,
        ended: Option<DateTime<Utc>>,
    }

    let mut where_clause = String::default();

    if params.id.is_some() || params.statuses.is_some() {
        let conditions = params
            .id
            .map(|_| "task_id = ?".to_string())
            .into_iter()
            .chain(params.statuses.as_ref().map(|statuses| {
                let binds = ",?".repeat(statuses.len()).chars().skip(1).collect::<String>();

                format!("status IN ({binds})")
            }))
            .join(" AND ");

        where_clause = format!("WHERE {conditions}");
    };

    let query_str = format!(
        "
        SELECT
          task_id,
          project_id,
          profile_id,
          repository_id,
          slug,
          package_id,
          arch,
          build_id,
          description,
          commit_ref,
          source_path,
          status,
          allocated_builder,
          log_path,
          started,
          updated,
          ended
        FROM task
        {where_clause};
        ",
    );

    let mut query = sqlx::query_as::<_, Row>(&query_str);

    if let Some(id) = params.id {
        query = query.bind(i64::from(id));
    }

    if let Some(statuses) = params.statuses {
        for status in statuses {
            query = query.bind(status.to_string());
        }
    }

    let rows = query.fetch_all(&mut *conn).await.context("fetch tasks")?;

    let mut tasks = rows
        .into_iter()
        .map(|row| Task {
            id: row.id,
            project_id: row.project_id,
            profile_id: row.profile_id,
            repository_id: row.repository_id,
            slug: row.slug,
            package_id: row.package_id,
            arch: row.arch,
            build_id: row.build_id,
            description: row.description,
            commit_ref: row.commit_ref,
            source_path: row.source_path,
            status: row.status,
            allocated_builder: row.allocated_builder,
            log_path: row.log_path,
            started: row.started,
            updated: row.updated,
            ended: row.ended,
            // Fetched next
            blocked_by: vec![],
        })
        .collect::<Vec<_>>();

    // max number of sqlite params
    for chunk in tasks.chunks_mut(32766) {
        let binds = ",?".repeat(chunk.len()).chars().skip(1).collect::<String>();

        let query_str = format!(
            "
            SELECT
              task_id,
              blocker
            FROM task_blockers
            WHERE task_id IN ({binds});
            ",
        );

        let mut query = sqlx::query_as::<_, (i64, String)>(&query_str);

        for task in chunk.iter() {
            query = query.bind(i64::from(task.id));
        }

        let rows = query.fetch_all(&mut *conn).await.context("fetch task blockers")?;

        for (id, blocker) in rows {
            if let Some(task) = chunk.iter_mut().find(|t| t.id == Id::from(id)) {
                task.blocked_by.push(blocker);
            }
        }
    }

    Ok(tasks)
}
