use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use futures::{select, FutureExt};
use serde::Deserialize;
use service::{endpoint::enrollment, signal, Role, Server, State};
use tracing::info;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config<VesselConfig>;

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

    let mut grpc = Server::new(Role::RepositoryManager, &config, &state)
        .enroll_with(config.domain.summit.clone())
        .start((host, port))
        .boxed()
        .fuse();
    let mut stop = signal::capture([signal::Kind::terminate(), signal::Kind::interrupt()])
        .boxed()
        .fuse();

    select! {
        res = grpc => res?,
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

#[derive(Debug, Clone, Deserialize)]
pub struct VesselConfig {
    pub summit: enrollment::Target,
}
