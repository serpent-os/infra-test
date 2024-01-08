use std::future::IntoFuture;
use std::{fs, path::PathBuf};

use axum::{extract::Request, middleware::Next, response::Response, Router};
use clap::Parser;
use futures::{select, FutureExt};
use log::{debug, info};
use serde::Deserialize;
use service::endpoint::enrollment::PendingEnrollment;
use service::{account, middleware, token, Database};
use service::{account::Admin, endpoint::enrollment};
use service::{crypto::KeyPair, endpoint};

mod api;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        host,
        web_port,
        grpc_port,
        db: db_path,
        config,
    } = Args::parse();

    let config = Config::load(&config)?;

    env_logger::init_from_env(
        env_logger::Env::new().default_filter_or(config.log_level.as_deref().unwrap_or("info")),
    );

    let app = Router::new()
        .nest("/api", api::router())
        .layer(axum::middleware::from_fn(log));

    let web_address = format!("{host}:{web_port}");
    let grpc_address = format!("{host}:{grpc_port}");

    // TODO: Persist
    let key_pair = KeyPair::generate();
    debug!("keypair generated: {}", key_pair.public_key().encode());
    let db = Database::new(&db_path).await?;
    debug!("database {db_path:?} opened");
    let pending_enrollment = PendingEnrollment::default();

    account::sync_admin(&db, config.admin.clone()).await?;

    let endpoint_service = endpoint::Server::new(endpoint::Service {
        issuer: enrollment::Issuer {
            key_pair: key_pair.clone(),
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
                req.extensions_mut().insert(db.clone());
                req.extensions_mut().insert(pending_enrollment.clone());
                Ok(req)
            },
        ))
        .layer(middleware::Log)
        .layer(middleware::Auth {
            pub_key: key_pair.public_key(),
            validation: token::Validation::new().iss(endpoint::Role::Hub.service_name()),
        })
        .add_service(endpoint_service)
        .serve(grpc_address.parse()?)
        .boxed()
        .fuse();

    let listener = tokio::net::TcpListener::bind(&web_address).await?;
    let mut web = axum::serve(listener, app).into_future().boxed().fuse();

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
    #[arg(long, default_value = "./summit.db")]
    db: PathBuf,
    #[arg(long, short, default_value = "./summit.toml")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    pub description: String,
    pub admin: Admin,
    pub log_level: Option<String>,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}

async fn log(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    debug!("Received request for {path}");

    let response = next.run(request).await;

    debug!("Returning response for {path}: {}", response.status());

    response
}
