use std::{convert::Infallible, future::Future, time::Duration};

use color_eyre::{Result, eyre::Context};
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};
use tracing::{Span, error, info};

use crate::{Manager, repository, task};

const TIMER_INTERVAL: Duration = Duration::from_secs(30);

pub type Sender = mpsc::UnboundedSender<Message>;

#[derive(Debug, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Message {
    AllocateBuilds,
    BuildSucceeded,
    BuildFailed,
    ImportSucceeded,
    ImportFailed,
    Timer(Instant),
}

pub async fn run(manager: Manager) -> Result<(Sender, impl Future<Output = Result<(), Infallible>> + use<>)> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Message>();

    let task = async move {
        while let Some(message) = receiver.recv().await {
            let kind = message.to_string();

            if let Err(e) = handle_message(&manager, message).await {
                let error = service::error::chain(e.as_ref() as &dyn std::error::Error);
                error!(message = kind, %error, "Error handling message");
            }
        }

        info!("Worker exiting");

        Ok(())
    };

    let _ = sender.send(Message::AllocateBuilds);

    Ok((sender, task))
}

pub async fn timer_task(sender: Sender) -> Result<(), Infallible> {
    let mut interval = time::interval(TIMER_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        let _ = sender.send(Message::Timer(interval.tick().await));
    }
}

async fn handle_message(manager: &Manager, message: Message) -> Result<()> {
    match message {
        Message::AllocateBuilds => allocate_builds().await,
        Message::BuildSucceeded => build_succeeded().await,
        Message::BuildFailed => build_failed().await,
        Message::ImportSucceeded => import_succeeded().await,
        Message::ImportFailed => import_failed().await,
        Message::Timer(_) => timer(manager).await,
    }
}

#[tracing::instrument(skip_all)]
async fn allocate_builds() -> Result<()> {
    info!("Allocating builds");

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn build_succeeded() -> Result<()> {
    info!("Build succeeded");

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn build_failed() -> Result<()> {
    info!("Build failed");

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn import_succeeded() -> Result<()> {
    info!("Import succeeded");

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn import_failed() -> Result<()> {
    info!("Import failed");

    Ok(())
}

#[tracing::instrument(skip_all, fields(project, repository))]
async fn timer(manager: &Manager) -> Result<()> {
    info!("Timer triggered");

    let span = Span::current();

    let mut have_changes = false;

    let mut conn = manager.acquire().await.context("acquire db conn")?;

    for project in manager.projects().await.context("list projects")? {
        span.record("project", &project.slug);

        for repo in &project.repositories {
            span.record("repository", &repo.name);

            let (mut repo, changed) = repository::refresh(&mut conn, &manager.state, repo.clone())
                .await
                .context("refresh repository")?;

            if changed {
                let repo_db = manager.repository_db(&repo.id)?.clone();

                repo = repository::reindex(&mut conn, &manager.state, repo, repo_db.clone())
                    .await
                    .context("reindex repository")?;

                task::create_missing(&mut conn, manager, &project, &repo, &repo_db)
                    .await
                    .context("create missing tasks")?;

                have_changes = true;
            }
        }
    }

    if have_changes {}

    Ok(())
}
