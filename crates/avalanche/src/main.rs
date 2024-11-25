use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use service::{Role, Server, State};
use tracing::info;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config;

use self::build::build;

mod api;
mod build;

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

    info!("avalanche listening on {host}:{port}");

    Server::new(Role::Builder, &config, &state)
        .merge_api(api::service(state.clone(), config.clone()))
        .serve_directory("/assets", "assets")
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
}
