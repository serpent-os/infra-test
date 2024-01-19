use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use color_eyre::eyre::Context;
use futures::{select, FutureExt};
use log::{debug, error, info};
use serde::Deserialize;
use service::{
    endpoint::{self, enrollment, Enrollment},
    logging, signal, Account, Endpoint, Role, State,
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

    tokio::spawn(send_initial_enrollment(config.clone(), state.clone()).map(log_error));

    info!("avalanche listening on {host}:{port}");

    let mut grpc = service::start((host, port), Role::Builder, config, state)
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

async fn log_error<T>(result: Result<T>) {
    if let Err(e) = result {
        error!("{e:#}");
    }
}

async fn send_initial_enrollment(config: Config, state: State) -> Result<()> {
    let target = &config.domain.summit;

    // If we're paired & operational, we don't need to resend
    for endpoint in Endpoint::list(&state.db).await? {
        let account = Account::get(&state.db, endpoint.account)
            .await
            .context(format!(
                "Can't find service account for endopoint {}",
                endpoint.id
            ))?;

        if matches!(endpoint.status, endpoint::Status::Operational)
            && endpoint.host_address == target.host_address
            && account.public_key == target.public_key.encode()
        {
            debug!(
                "Configured endpoint {} already operational with public key {}",
                endpoint.host_address, account.public_key
            );
            return Ok(());
        }
    }

    let issuer = config.issuer(Role::Builder, state.key_pair.clone());
    let enrollment = Enrollment::send(config.domain.summit, issuer)
        .await
        .context("Failed to send enrollment")?;
    state
        .pending_enrollment
        .insert(enrollment.endpoint, enrollment)
        .await;

    Ok(())
}
