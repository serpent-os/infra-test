use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use futures::{select, FutureExt};
use service::{signal, Role, Server, State};
use tracing::info;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config;

use self::collection_db::CollectionDb;

mod api;
mod collection_db;
mod worker;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        host,
        port,
        config,
        root,
    } = Args::parse();

    let config = Config::load(config.unwrap_or_else(|| root.join("config.toml"))).await?;

    service::tracing::init(&config.tracing);

    let state = State::load(root).await?;

    info!("vessel listening on {host}:{port}");

    let (worker_sender, worker_task) = worker::run(&state).await?;

    let mut http = Server::new(Role::RepositoryManager, &config, &state)
        .merge_api(api::service(state.db.clone(), worker_sender))
        .start((host, port))
        .boxed()
        .fuse();

    let mut stop = signal::capture([signal::Kind::terminate(), signal::Kind::interrupt()])
        .boxed()
        .fuse();

    select! {
        res = worker_task.fuse() => res?,
        res = http => res?,
        res = stop => res?,
    }

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
}
