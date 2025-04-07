use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use color_eyre::eyre::Context;
use service::{Role, Server, State};
use tracing::info;

pub use self::manager::Manager;
pub use self::profile::Profile;
pub use self::project::Project;
pub use self::queue::Queue;
pub use self::repository::Repository;
pub use self::seed::seed;
pub use self::task::Task;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config;

mod api;
mod manager;
mod profile;
mod project;
mod queue;
mod repository;
mod seed;
mod task;
mod worker;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        host,
        port,
        config,
        root,
        seed_from,
    } = Args::parse();

    let config = Config::load(config.unwrap_or_else(|| root.join("config.toml"))).await?;

    service::tracing::init(&config.tracing);

    let state = State::load(root)
        .await?
        .with_migrations(sqlx::migrate!("./migrations"))
        .await?;

    if let Some(from_path) = seed_from {
        seed(&state, from_path).await.context("seeding")?;
    }

    let manager = Manager::load(state.clone()).await.context("load manager")?;

    let (worker_sender, worker_task) = worker::run(manager).await?;

    info!("summit listening on {host}:{port}");

    Server::new(Role::Hub, &config, &state)
        .merge_api(api::service(state.clone(), worker_sender.clone()))
        .with_task("worker", worker_task)
        .start((host, port))
        .await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long, default_value = "5003")]
    port: u16,
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long, short, default_value = ".")]
    root: PathBuf,
    #[arg(long = "seed")]
    seed_from: Option<PathBuf>,
}
