//! Batteries included server that provides common service APIs
//! over http, with the ability to handle additional consumer
//! defined APIs
use std::{io, path::Path};

use thiserror::Error;
use tokio::net::ToSocketAddrs;
use tracing::error;

use crate::{
    account, api,
    endpoint::{self, enrollment},
    error, middleware, token, Config, Role, State,
};

/// Start the [`Server`] without additional configuration
pub async fn start(addr: impl ToSocketAddrs, role: Role, config: &Config, state: &State) -> Result<(), Error> {
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
}

impl<'a> Server<'a> {
    /// Create a new [`Server`]
    pub fn new(role: Role, config: &'a Config, state: &'a State) -> Self {
        // All services have an endpoint service
        let endpoint_service = endpoint::service(role, config, state);
        let router = axum::Router::new().merge(endpoint_service.into_router());

        Self {
            router,
            config,
            state,
            role,
            extract_token: middleware::ExtractToken {
                pub_key: state.key_pair.public_key(),
                validation: token::Validation::new().iss(role.service_name()),
            },
        }
    }
}

impl<'a> Server<'a> {
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

    /// Start the server and perform the following:
    ///
    /// - Sync the defined [`Config::admin`] to the service [`Database`] to ensure
    ///   it's credentials can authenticate and hit all admin endpoints.
    /// - Send auto-enrollment for all [`Config::downstream`] targets defined when [`Role::Hub`]
    /// - Start the underlying server to handle endpoint API routes
    ///   and any additional API routes added via [`Server::merge_api`].
    ///
    /// [`Database`]: crate::Database
    pub async fn start(self, addr: impl ToSocketAddrs) -> Result<(), Error> {
        account::sync_admin(&self.state.db, self.config.admin.clone()).await?;

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

        let app = axum::Router::new()
            .nest("/", self.router)
            .layer(self.extract_token)
            .layer(middleware::Log);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

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
