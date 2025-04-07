use std::collections::{HashMap, VecDeque};

use color_eyre::eyre::{self, Context, OptionExt, Result};
use moss::db::meta;
use service::{Collectable, Endpoint, State, database::Transaction, endpoint};
use sqlx::{Sqlite, pool::PoolConnection};
use tokio::task::spawn_blocking;
use tracing::{Span, info, warn};

use crate::{Project, Queue, profile, project, repository, task};

pub struct Manager {
    pub state: State,
    queue: Queue,
    profile_dbs: HashMap<profile::Id, meta::Database>,
    repository_dbs: HashMap<repository::Id, meta::Database>,
}

impl Manager {
    #[tracing::instrument(name = "load_manager", skip_all)]
    pub async fn load(state: State) -> Result<Self> {
        let projects = project::list(&mut *state.service_db.acquire().await?).await?;

        let span = Span::current();

        // Moss DB implementations are blocking
        let (state, projects, profile_dbs, repository_dbs) = spawn_blocking(move || {
            let _enter = span.enter();

            let profile_dbs = projects
                .iter()
                .flat_map(|project| {
                    project
                        .profiles
                        .iter()
                        .map(|profile| Ok((profile.id, connect_profile_db(&state, &profile.id)?)))
                })
                .collect::<Result<HashMap<_, _>, eyre::Error>>()?;

            let repository_dbs = projects
                .iter()
                .flat_map(|project| {
                    project
                        .repositories
                        .iter()
                        .map(|repo| Ok((repo.id, connect_repository_db(&state, &repo.id)?)))
                })
                .collect::<Result<HashMap<_, _>, eyre::Error>>()?;

            info!(num_projects = projects.len(), "Projects loaded");

            Result::<_, eyre::Error>::Ok((state, projects, profile_dbs, repository_dbs))
        })
        .await
        .context("join handle")??;

        let mut manager = Self {
            state,
            queue: Queue::default(),
            profile_dbs,
            repository_dbs,
        };

        let mut conn = manager.acquire().await.context("acquire db conn")?;

        for project in &projects {
            // Refresh all profiles
            for profile in &project.profiles {
                let db = manager.profile_db(&profile.id).cloned()?;

                profile::refresh(&manager.state, profile, db)
                    .await
                    .context("refresh profile")?;
            }

            // Add all missing tasks
            for repo in &project.repositories {
                let db = manager.repository_db(&repo.id)?;

                task::create_missing(&mut conn, &manager, project, repo, db)
                    .await
                    .context("create missing tasks")?;
            }
        }

        manager
            .queue
            .recompute(&mut conn, &projects, &manager.repository_dbs)
            .await
            .context("recompute queue")?;

        Ok(manager)
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

    pub async fn allocate_builds(&mut self) -> Result<()> {
        let mut conn = self.acquire().await.context("acquire db conn")?;

        let projects = project::list(&mut conn).await.context("list projects")?;

        self.queue
            .recompute(&mut conn, &projects, &self.repository_dbs)
            .await
            .context("recompute queue")?;

        let mut available = self.queue.available().collect::<VecDeque<_>>();
        let mut next_task = available.pop_front();

        let builders = Endpoint::list(&mut *conn)
            .await
            .context("list endpoints")?
            .into_iter()
            .filter(Endpoint::is_idle_builder);

        // We will use a TX within each build to atomically
        // update all the things together, so we don't need
        // this connection anymore
        drop(conn);

        for mut builder in builders {
            if let Some(task) = next_task {
                match task::build(&self.state, &mut builder, task).await {
                    Ok(_) => {
                        next_task = available.pop_front();
                    }
                    Err(_) => {
                        warn!(builder = %builder.id, "Failed to send build, trying next builder");
                    }
                }
            } else {
                // Nothing else to build
                break;
            }
        }

        Ok(())
    }

    pub async fn build_succeeded(
        &mut self,
        task_id: task::Id,
        builder: endpoint::Id,
        collectables: Vec<Collectable>,
    ) -> Result<bool> {
        let publishing_failed = task::build::succeeded(&self.state, task_id, builder, collectables)
            .await
            .context("set task as build succeeded")?;

        if publishing_failed {
            let mut tx = self.begin().await.context("begin db tx")?;

            let projects = project::list(tx.as_mut()).await.context("list projects")?;

            self.queue
                .task_failed(&mut tx, task_id)
                .await
                .context("add queue blockers")?;

            self.queue
                .recompute(tx.as_mut(), &projects, &self.repository_dbs)
                .await
                .context("recompute queue")?;

            tx.commit().await.context("commit db tx")?;
        }

        Ok(publishing_failed)
    }

    pub async fn build_failed(
        &mut self,
        task_id: task::Id,
        builder: endpoint::Id,
        collectables: Vec<Collectable>,
    ) -> Result<()> {
        task::build::failed(&self.state, task_id, builder, collectables)
            .await
            .context("set task as build failed")?;

        let mut tx = self.begin().await.context("begin db tx")?;

        let projects = project::list(tx.as_mut()).await.context("list projects")?;

        self.queue
            .task_failed(&mut tx, task_id)
            .await
            .context("add queue blockers")?;

        self.queue
            .recompute(tx.as_mut(), &projects, &self.repository_dbs)
            .await
            .context("recompute queue")?;

        tx.commit().await.context("commit db tx")?;

        Ok(())
    }

    pub async fn import_succeeded(&mut self, task_id: task::Id) -> Result<()> {
        let mut tx = self.begin().await.context("begin db tx")?;

        task::set_status(&mut tx, task_id, task::Status::Completed)
            .await
            .context("set task as import failed")?;

        let projects = project::list(tx.as_mut()).await.context("list projects")?;

        let task = task::query(tx.as_mut(), task::query::Params::default().id(task_id))
            .await
            .context("query task")?
            .into_iter()
            .next()
            .ok_or_eyre("task is missing")?;

        self.queue
            .task_completed(&mut tx, task_id)
            .await
            .context("add queue blockers")?;

        self.queue
            .recompute(tx.as_mut(), &projects, &self.repository_dbs)
            .await
            .context("recompute queue")?;

        tx.commit().await.context("commit db tx")?;

        let profile = projects
            .iter()
            .find_map(|p| p.profiles.iter().find(|p| p.id == task.profile_id))
            .ok_or_eyre("missing profile")?;
        let profile_db = self
            .profile_dbs
            .get(&task.profile_id)
            .ok_or_eyre("missing profile db")?
            .clone();
        profile::refresh(&self.state, profile, profile_db)
            .await
            .context("refresh profile")?;

        Ok(())
    }

    pub async fn import_failed(&mut self, task_id: task::Id) -> Result<()> {
        let mut tx = self.begin().await.context("begin db tx")?;

        task::set_status(&mut tx, task_id, task::Status::Failed)
            .await
            .context("set task as import failed")?;

        let projects = project::list(tx.as_mut()).await.context("list projects")?;

        self.queue
            .task_failed(&mut tx, task_id)
            .await
            .context("add queue blockers")?;

        self.queue
            .recompute(tx.as_mut(), &projects, &self.repository_dbs)
            .await
            .context("recompute queue")?;

        tx.commit().await.context("commit db tx")?;

        Ok(())
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
