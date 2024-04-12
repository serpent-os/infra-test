use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use futures::{select, FutureExt};
use service::{signal, Role, State};
use tracing::info;

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
        assets,
        root,
    } = Args::parse();

    let assets = assets.unwrap_or_else(|| root.join("assets"));
    let config = Config::load(config.unwrap_or_else(|| root.join("config.toml"))).await?;

    service::tracing::init(&config.tracing);

    let state = State::load(root).await?;

    let mut web = web::serve((host, web_port), assets, state.clone())
        .await?
        .fuse();
    let mut grpc = service::start((host, grpc_port), Role::Hub, &config, &state)
        .boxed()
        .fuse();
    let mut stop = signal::capture([signal::Kind::terminate(), signal::Kind::interrupt()])
        .boxed()
        .fuse();

    info!("summit listening on web: {host}:{web_port}, grpc: {host}:{grpc_port}");

    select! {
        res = grpc => res?,
        res = web => res?,
        res = stop => res?,
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
    #[arg(long, short)]
    assets: Option<PathBuf>,
    #[arg(long, short, default_value = ".")]
    root: PathBuf,
}
