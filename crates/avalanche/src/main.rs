use std::{fs, path::PathBuf};

use clap::Parser;
use log::{debug, error, info};
use serde::Deserialize;
use service::{
    account::{self, Admin},
    crypto::KeyPair,
    endpoint::{
        self,
        enrollment::{self, PendingEnrollment},
        Enrollment,
    },
    middleware, token, Database,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        host,
        port,
        db: db_path,
        config,
    } = Args::parse();

    let config = Config::load(&config)?;

    env_logger::init_from_env(
        env_logger::Env::new().default_filter_or(config.log_level.as_deref().unwrap_or("info")),
    );

    let address = format!("{host}:{port}");

    // TODO: Persist
    let key_pair = KeyPair::generate();
    debug!("keypair generated: {}", key_pair.public_key().encode());
    let db = Database::new(&db_path).await?;
    debug!("database {db_path:?} opened");
    let pending_enrollment = PendingEnrollment::default();

    account::sync_admin(&db, config.admin.clone()).await?;

    let issuer = enrollment::Issuer {
        key_pair: key_pair.clone(),
        // TODO: Domain name when deployed
        host_address: format!("http://{address}").parse()?,
        role: endpoint::Role::Builder,
        admin_name: config.admin.username.clone(),
        admin_email: config.admin.email.clone(),
        description: config.description.clone(),
    };

    let endpoint_service = endpoint::Server::new(endpoint::Service {
        issuer: issuer.clone(),
    });

    tokio::spawn({
        let pending_enrollment = pending_enrollment.clone();

        async move {
            match Enrollment::send(config.summit, issuer).await {
                Ok(enrollment) => {
                    pending_enrollment
                        .insert(enrollment.endpoint, enrollment)
                        .await;
                }
                Err(err) => {
                    error!("Failed to send enrollment: {err}");
                }
            }
        }
    });

    info!("avalanche listening on {address}");

    tonic::transport::Server::builder()
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
            validation: token::Validation::new().iss(endpoint::Role::Builder.service_name()),
        })
        .add_service(endpoint_service)
        .serve(address.parse()?)
        .await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value = "5002")]
    port: u16,
    #[arg(long, default_value = "./avalanche.db")]
    db: PathBuf,
    #[arg(long, short, default_value = "./avalanche.toml")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    pub description: String,
    pub admin: Admin,
    pub log_level: Option<String>,
    pub summit: enrollment::Target,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }
}
