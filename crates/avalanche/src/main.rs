use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use log::{error, info};
use serde::Deserialize;
use service::{
    endpoint::{enrollment, Enrollment, Role},
    logging, State,
};

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;
pub type Config = service::Config<AvalancheConfig>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        host,
        port,
        config,
        root,
    } = Args::parse();

    let config = Config::load(config.unwrap_or_else(|| root.join("config.toml"))).await?;

    logging::init(&config);

    let state = State::load(root).await?;

    tokio::spawn(send_initial_enrollment(config.clone(), state.clone()));

    info!("avalanche listening on {host}:{port}");

    service::start((host, port), Role::Builder, config, state).await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long, default_value = "5002")]
    port: u16,
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long, short, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AvalancheConfig {
    pub summit: enrollment::Target,
}

async fn send_initial_enrollment(config: Config, state: State) {
    let issuer = config.issuer(Role::Builder, state.key_pair.clone());

    // TODO: Check DB to ensure we aren't already enrolled to this
    // summit target
    match Enrollment::send(config.domain.summit, issuer).await {
        Ok(enrollment) => {
            state
                .pending_enrollment
                .insert(enrollment.endpoint, enrollment)
                .await;
        }
        Err(err) => {
            error!("Failed to send enrollment: {err}");
        }
    }
}
