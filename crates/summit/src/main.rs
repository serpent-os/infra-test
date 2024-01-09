use std::{fs, path::PathBuf};

use clap::Parser;
use futures::{select, FutureExt};
use log::info;
use serde::Deserialize;
use service::endpoint;
use service::{account, middleware, token, State};
use service::{account::Admin, endpoint::enrollment};

mod web;

pub type Result<T, E = color_eyre::eyre::Error> = std::result::Result<T, E>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        host,
        web_port,
        grpc_port,
        config,
        root,
    } = Args::parse();

    let config = Config::load(&config.unwrap_or_else(|| root.join("config.toml")))?;

    env_logger::Builder::from_env(
        env_logger::Env::new().default_filter_or(config.log_level.as_deref().unwrap_or("info")),
    )
    .format_module_path(false)
    .init();

    let web_address = format!("{host}:{web_port}");
    let grpc_address = format!("{host}:{grpc_port}");

    let state = State::load(root).await?;

    account::sync_admin(&state.db, config.admin.clone()).await?;

    let endpoint_service = endpoint::Server::new(endpoint::Service {
        issuer: enrollment::Issuer {
            key_pair: state.key_pair.clone(),
            // TODO: Domain name when deployed
            host_address: format!("http://{web_address}").parse()?,
            role: endpoint::Role::Hub,
            admin_name: config.admin.username.clone(),
            admin_email: config.admin.email.clone(),
            description: config.description.clone(),
        },
    });

    let mut grpc = tonic::transport::Server::builder()
        .layer(tonic::service::interceptor(
            move |mut req: tonic::Request<()>| {
                req.extensions_mut().insert(state.db.clone());
                req.extensions_mut()
                    .insert(state.pending_enrollment.clone());
                Ok(req)
            },
        ))
        .layer(middleware::Log)
        .layer(middleware::Auth {
            pub_key: state.key_pair.public_key(),
            validation: token::Validation::new().iss(endpoint::Role::Hub.service_name()),
        })
        .add_service(endpoint_service)
        .serve(grpc_address.parse()?)
        .boxed()
        .fuse();

    let mut web = web::serve(&web_address).await?.fuse();

    info!("summit listening on web: {web_address}, grpc: {grpc_address}");

    select! {
        res = grpc => res?,
        res = web => res?,
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value = "5000")]
    web_port: u16,
    #[arg(long, default_value = "5001")]
    grpc_port: u16,
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long, short, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    pub description: String,
    pub admin: Admin,
    pub log_level: Option<String>,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}
