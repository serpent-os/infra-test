use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use derive_more::derive::{Display, From, Into};
use moss::{db::meta, dependency, package::Meta};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, prelude::FromRow};
use tokio::task;
use tracing::{debug, warn};

use crate::{Manager, Project, Repository, profile, project, repository};

pub use self::create::create;

pub mod create;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, From, Into, Display, FromRow)]
pub struct Id(i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: Id,
    pub project_id: project::Id,
    pub profile_id: profile::Id,
    pub repository_id: repository::Id,
    pub slug: String,
    pub package_id: String,
    pub arch: String,
    pub build_id: String,
    pub description: String,
    pub commit_ref: String,
    pub source_path: String,
    pub status: Status,
    pub allocated_builder: Option<String>,
    pub log_path: Option<String>,
    pub started: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub ended: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
    /// Freshly created task
    New,
    /// Failed execution or evaluation
    Failed,
    /// This task is now building
    Building,
    /// Now publishing to Vessel
    Publishing,
    /// Job successfully completed!
    Completed,
    /// This build must remain blocked until its block
    /// criteria have been met, i.e. the dependent that
    /// caused the failure has been fixed.
    Blocked,
}

#[tracing::instrument(name = "create_missing_tasks", skip_all)]
pub async fn create_missing(
    conn: &mut SqliteConnection,
    manager: &Manager,
    project: &Project,
    repo: &Repository,
    repo_db: &meta::Database,
) -> Result<()> {
    for profile in &project.profiles {
        let profile_db = manager.profile_db(&profile.id).context("missing profile db")?;

        let packages = task::spawn_blocking({
            let repo_db = repo_db.clone();
            move || repo_db.query(None)
        })
        .await
        .context("join handle")?
        .context("list source repo packages")?;

        for (_, meta) in packages {
            'providers: for name in meta
                .providers
                .iter()
                .filter(|p| p.kind == dependency::Kind::PackageName)
                .cloned()
            {
                let corresponding = task::spawn_blocking({
                    let profile_db = profile_db.clone();
                    move || profile_db.query(Some(meta::Filter::Provider(name)))
                })
                .await
                .context("join handle")?
                .context("list package dependents")?;

                let slug = || format!("~/{}/{}/{}", project.slug, repo.name, meta.name);
                let version = |meta: &Meta| format!("{}-{}", meta.version_identifier, meta.source_release);

                let latest = corresponding.iter().max_by(|(_, a), (_, b)| {
                    a.version_identifier
                        .cmp(&b.version_identifier)
                        .then(a.source_release.cmp(&b.source_release))
                });

                if let Some((_, published)) = latest {
                    if published.source_release >= meta.source_release {
                        warn!(
                            slug = slug(),
                            published = version(published),
                            recipe = version(&meta),
                            "Newer package already in index"
                        );

                        continue 'providers;
                    } else {
                        debug!(
                            slug = slug(),
                            published = version(published),
                            recipe = version(&meta),
                            "Adding newer package as task"
                        );

                        create(
                            conn,
                            project,
                            profile,
                            repo,
                            &meta,
                            format!(
                                "Update {} from {} to {}",
                                meta.source_id,
                                version(published),
                                version(&meta)
                            ),
                        )
                        .await
                        .context("create task")?;
                    }
                } else {
                    debug!(
                        slug = slug(),
                        version = version(&meta),
                        "Adding missing package as task"
                    );

                    create(
                        conn,
                        project,
                        profile,
                        repo,
                        &meta,
                        format!("Initial build of {} ({})", meta.source_id, version(&meta)),
                    )
                    .await
                    .context("create task")?;

                    break 'providers;
                };
            }
        }
    }

    Ok(())
}
