//! Batteries included server that provides common service APIs
//! over http, with the ability to handle additional consumer
//! defined APIs
use std::{future::IntoFuture, io, net::IpAddr, path::Path, time::Duration};

use thiserror::Error;
use tracing::{error, info};

use crate::{Config, Role, State, account, api, endpoint::enrollment, error, middleware, signal, task, token};

pub use crate::task::CancellationToken;

/// Start the [`Server`] without additional configuration
pub async fn start(addr: (IpAddr, u16), role: Role, config: &Config, state: &State) -> Result<(), Error> {
    Server::new(role, config, state).start(addr).await
}

/// Routes http requests through logging & auth middlewares to common service API handlers by default, with
/// the ability to handle additional consumer defined APIs via [`Server::merge_api`].
pub struct Server<'a> {
    router: axum::Router,
    config: &'a Config,
    state: &'a State,
    role: Role,
    extract_token: middleware::ExtractToken,
    signals: Vec<signal::Kind>,
    runner: task::Runner,
}

impl<'a> Server<'a> {
    /// Create a new [`Server`]
    pub fn new(role: Role, config: &'a Config, state: &'a State) -> Self {
        let shared_services = api::v1::services(role, config, state);
        let router = axum::Router::new().merge(shared_services.into_router());

        Self {
            router,
            config,
            state,
            role,
            extract_token: middleware::ExtractToken {
                pub_key: state.key_pair.public_key(),
                validation: token::Validation::new().iss(role.service_name()),
            },
            signals: vec![signal::Kind::terminate(), signal::Kind::interrupt()],
            runner: task::Runner::new(),
        }
    }
}

impl Server<'_> {
    /// Override the default graceful shutdown duration (5s)
    pub fn with_graceful_shutdown(self, duration: Duration) -> Self {
        Self {
            runner: self.runner.with_graceful_shutdown(duration),
            ..self
        }
    }

    /// Add a task which is killed immediately upon shutdown sequence
    pub fn with_task<F, E>(self, name: &'static str, task: F) -> Self
    where
        F: IntoFuture<Output = Result<(), E>>,
        F::IntoFuture: Send + 'static,
        E: std::error::Error + Send + 'static,
    {
        Self {
            runner: self.runner.with_task(name, task),
            ..self
        }
    }

    /// Add a task which can monitor shutdown sequence via [`CancellationToken`].
    /// The task is given graceful shutdown duration to cleanup & exit before being
    /// forcefully killed.
    pub fn with_cancellation_task<F, E>(self, name: &'static str, f: impl FnOnce(CancellationToken) -> F) -> Self
    where
        F: IntoFuture<Output = Result<(), E>>,
        F::IntoFuture: Send + 'static,
        E: std::error::Error + Send + 'static,
    {
        Self {
            runner: self.runner.with_cancellation_task(name, f),
            ..self
        }
    }

    /// Merges an [`api::Service`] with the server
    pub fn merge_api(self, service: api::Service) -> Self {
        Self {
            router: self.router.merge(service.into_router()),
            ..self
        }
    }

    /// Merges an [`axum::Router`] with the server
    pub fn merge(self, router: impl Into<axum::Router>) -> Self {
        Self {
            router: self.router.merge(router),
            ..self
        }
    }

    /// Serve static files under `route` from the provided `directory`
    pub fn serve_directory(self, route: &str, directory: impl AsRef<Path>) -> Self {
        Self {
            router: self.router.nest_service(
                route,
                tower_http::services::ServeDir::new(directory).precompressed_gzip(),
            ),
            ..self
        }
    }

    /// Serve static files from the provided `directory` as fallback
    pub fn serve_fallback_directory(self, directory: impl AsRef<Path>) -> Self {
        Self {
            router: self.router.fallback_service(
                tower_http::services::ServeDir::new(directory.as_ref())
                    .precompressed_gzip()
                    .fallback(tower_http::services::ServeFile::new(
                        directory.as_ref().join("404.html"),
                    )),
            ),
            ..self
        }
    }

    /// Start the server and perform the following:
    ///
    /// - Sync the defined [`Config::admin`] to the service [`Database`] to ensure
    ///   it's credentials can authenticate and hit all admin endpoints.
    /// - Send auto-enrollment for all [`Config::downstream`] targets defined when [`Role::Hub`]
    /// - Start the underlying server to handle endpoint API routes
    ///   and any additional API routes added via [`Server::merge_api`].
    ///
    /// [`Database`]: crate::Database
    pub async fn start(self, addr: (IpAddr, u16)) -> Result<(), Error> {
        account::sync_admin(&self.state.service_db, self.config.admin.clone()).await?;

        if self.role == Role::Hub {
            if let Err(e) = enrollment::auto_enrollment(
                &self.config.downstream,
                self.config.issuer(self.role, self.state.key_pair.clone()),
                self.state,
            )
            .await
            {
                error!(error = %error::chain(e), "Auto enrollment failed");
            }
        }

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let router = self.router.layer(self.extract_token).layer(middleware::Log);

        self.runner
            .with_task("http server", async move {
                let (host, port) = addr;

                info!("listening on {host}:{port}");

                axum::serve(listener, router).await
            })
            .with_task("signal capture", signal::capture(self.signals))
            .run()
            .await;

        Ok(())
    }
}

/// A server error
#[derive(Debug, Error)]
pub enum Error {
    /// Syncing admin account failed
    #[error("sync admin account")]
    SyncAdmin(#[from] account::Error),
    /// Axum IO error
    #[error(transparent)]
    Serve(#[from] io::Error),
}
