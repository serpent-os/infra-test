use std::{convert::Infallible, future::Future, time::Duration};

use color_eyre::{Result, eyre::Context};
use service::State;
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};
use tracing::{error, info};

use crate::Manager;

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

pub async fn run(state: &State) -> Result<(Sender, impl Future<Output = Result<(), Infallible>> + use<>)> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Message>();

    let manager = Manager::new(state).await.context("create manager")?;

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
        Message::AllocateBuilds => allocate_builds(manager).await,
        Message::BuildSucceeded => build_succeeded().await,
        Message::BuildFailed => build_failed().await,
        Message::ImportSucceeded => import_succeeded().await,
        Message::ImportFailed => import_failed().await,
        Message::Timer(_) => timer().await,
    }
}

#[tracing::instrument(skip_all)]
async fn allocate_builds(manager: &Manager) -> Result<()> {
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

#[tracing::instrument(skip_all)]
async fn timer() -> Result<()> {
    info!("Timer triggered");

    Ok(())
}
