use std::collections::HashMap;

use color_eyre::eyre::{self, Context, OptionExt, Result};
use moss::db::meta;
use service::{State, database::Transaction};
use sqlx::{Sqlite, pool::PoolConnection};
use tokio::task;
use tracing::{Span, info};

use crate::{Project, profile, project, repository};

pub struct Manager {
    pub state: State,
    profile_dbs: HashMap<profile::Id, meta::Database>,
    repository_dbs: HashMap<repository::Id, meta::Database>,
}

impl Manager {
    #[tracing::instrument(name = "load_manager", skip_all)]
    pub async fn load(state: State) -> Result<Self> {
        let projects = project::list(&mut *state.service_db.acquire().await?).await?;

        let span = Span::current();

        // Moss DB implementations are blocking
        task::spawn_blocking(move || {
            let _enter = span.enter();

            let profile_dbs = projects
                .iter()
                .flat_map(|project| {
                    project
                        .profiles
                        .iter()
                        .map(|profile| Ok((profile.id, connect_profile_db(&state, &profile.id)?)))
                })
                .collect::<Result<_, eyre::Error>>()?;

            let repository_dbs = projects
                .iter()
                .flat_map(|project| {
                    project
                        .repositories
                        .iter()
                        .map(|repo| Ok((repo.id, connect_repository_db(&state, &repo.id)?)))
                })
                .collect::<Result<_, eyre::Error>>()?;

            info!(num_projects = projects.len(), "Projects loaded");

            Ok(Self {
                state,
                profile_dbs,
                repository_dbs,
            })
        })
        .await
        .context("join handle")?
    }

    pub async fn begin(&self) -> Result<Transaction> {
        Ok(self.state.service_db.begin().await?)
    }

    pub async fn acquire(&self) -> Result<PoolConnection<Sqlite>> {
        Ok(self.state.service_db.acquire().await?)
    }

    pub async fn projects(&self) -> Result<Vec<Project>> {
        Ok(project::list(&mut *self.state.service_db.acquire().await?).await?)
    }

    pub fn profile_db(&self, profile: &profile::Id) -> Result<&meta::Database> {
        self.profile_dbs.get(profile).ok_or_eyre("missing profile")
    }

    pub fn repository_db(&self, repo: &repository::Id) -> Result<&meta::Database> {
        self.repository_dbs.get(repo).ok_or_eyre("missing repository")
    }
}

fn connect_profile_db(state: &State, profile: &profile::Id) -> Result<meta::Database> {
    use std::fs;

    let parent = state.db_dir.join("profile");

    fs::create_dir_all(&parent).context("create profile db directory")?;

    let db =
        meta::Database::new(parent.join(profile.to_string()).to_string_lossy().as_ref()).context("open profile db")?;

    Ok(db)
}

fn connect_repository_db(state: &State, repository: &repository::Id) -> Result<meta::Database> {
    use std::fs;

    let parent = state.db_dir.join("repository");

    fs::create_dir_all(&parent).context("create repository db directory")?;

    let db = meta::Database::new(parent.join(repository.to_string()).to_string_lossy().as_ref())
        .context("open repository db")?;

    Ok(db)
}
