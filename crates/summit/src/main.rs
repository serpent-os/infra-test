use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use futures::{select, FutureExt};
use log::info;
use service::{endpoint::Role, logging, State};

mod web;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config<()>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        host,
        web_port,
        grpc_port,
        config,
        root,
    } = Args::parse();

    let config = Config::load(config.unwrap_or_else(|| root.join("config.toml"))).await?;

    logging::init(&config);

    let state = State::load(root).await?;

    let mut web = web::serve((host, web_port)).await?.fuse();
    let mut grpc = service::start((host, grpc_port), Role::Hub, config, state)
        .boxed()
        .fuse();

    info!("summit listening on web: {host}:{web_port}, grpc: {host}:{grpc_port}");

    select! {
        res = grpc => res?,
        res = web => res?,
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long, default_value = "5000")]
    web_port: u16,
    #[arg(long, default_value = "5001")]
    grpc_port: u16,
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long, short, default_value = ".")]
    root: PathBuf,
}
