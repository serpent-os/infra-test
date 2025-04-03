use color_eyre::eyre::{Context, Result};
use service::{Database, State};

use crate::{Project, project};

pub struct Manager {
    db: Database,
    projects: Vec<Project>,
}

impl Manager {
    pub async fn new(state: &State) -> Result<Self> {
        let projects = project::list(&mut *state.service_db.acquire().await.context("acquire db connection")?)
            .await
            .context("list projects")?;

        dbg!(&projects);

        Ok(Self {
            db: state.service_db.clone(),
            projects,
        })
    }
}
