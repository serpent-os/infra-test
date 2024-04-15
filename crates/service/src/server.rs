//! Batteries included server that provides [`account::Service`] & [`endpoint::Service`]
//! over gRPC, with the ability to handle additional consumer defined services.
use std::{convert::Infallible, net::SocketAddr};

use futures::TryFutureExt;
use http::{Request, Response};
use thiserror::Error;
use tonic::{
    body::BoxBody,
    transport::{self, server::Routes, Body, NamedService},
};
use tower::{
    layer::util::{Identity, Stack},
    Layer, Service,
};
use tracing::error;

use crate::{
    account,
    endpoint::{
        self,
        enrollment::{self, send_initial_enrollment},
    },
    error, middleware, token, Config, Role, State,
};

/// Start the [`Server`] without additional configuration
pub async fn start<T>(bind: impl Into<SocketAddr>, role: Role, config: &Config<T>, state: &State) -> Result<(), Error> {
    Server::new(role, config, state).start(bind).await
}

/// Default gRPC middleware provided by [`Server`] providing auth and logging
pub type DefaultMiddleware = Stack<middleware::Auth, Stack<middleware::Log, Identity>>;

/// Routes gRPC requests through [`DefaultMiddleware`] to [`account::Service`] and [`endpoint::Service`] handlers by default, with
/// the ability to handle additional consumer defined services via [`Server::add_service`].
pub struct Server<'a, T, L> {
    router: transport::server::Router<L>,
    config: &'a Config<T>,
    state: &'a State,
    role: Role,
    enroll_with: Option<enrollment::Target>,
}

impl<'a, T> Server<'a, T, DefaultMiddleware> {
    /// Create a new [`Server`]
    pub fn new(role: Role, config: &'a Config<T>, state: &'a State) -> Self {
        let endpoint_service = endpoint::Server::new(endpoint::Service {
            issuer: config.issuer(role, state.key_pair.clone()),
            db: state.db.clone(),
            pending_enrollment: state.pending_enrollment.clone(),
        });
        let account_service = account::Server::new(account::Service {
            db: state.db.clone(),
            key_pair: state.key_pair.clone(),
            role,
        });
        let router = tonic::transport::Server::builder()
            .layer(middleware::Log)
            .layer(middleware::Auth {
                pub_key: state.key_pair.public_key(),
                validation: token::Validation::new().iss(role.service_name()),
            })
            .add_service(endpoint_service)
            .add_service(account_service);

        Self {
            router,
            config,
            state,
            role,
            enroll_with: None,
        }
    }

    /// Specify an [`enrollment::Target`] to enroll with. If not yet operational,
    /// an enrollment request will be sent to the target service on [`Server::start`].
    pub fn enroll_with(self, target: enrollment::Target) -> Self {
        Self {
            enroll_with: Some(target),
            ..self
        }
    }
}

impl<'a, T, L> Server<'a, T, L>
where
    L: Layer<Routes>,
    L::Service: Service<Request<Body>, Response = Response<BoxBody>> + Clone + Send + 'static,
    <L::Service as Service<Request<Body>>>::Future: Send + 'static,
    <L::Service as Service<Request<Body>>>::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    /// Add a custom tower [`Service`] to the server. This can be used by consumers
    /// to add additional services, such as custom tonic gRPC service handlers.
    pub fn add_service<S>(self, service: S) -> Self
    where
        S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible>
            + NamedService
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        Self {
            router: self.router.add_service(service),
            ..self
        }
    }

    /// Start the server and perform the following:
    ///
    /// - Sync the defined [`Config::admin`] to the service [`Database`] to ensure
    /// it's credentials can authenticate and hit all admin endpoints.
    /// - Send an initial enrollment request, if applicable, when [`Server::enroll_with`]
    /// is used.
    /// - Start the underlying gRPC server to handle [`account::Service`] & [`endpoint::Service`] routes,
    /// and any custom service added via [`Server::add_service`].
    ///
    /// [`Database`]: crate::Database
    pub async fn start(self, bind: impl Into<SocketAddr>) -> Result<(), Error> {
        account::sync_admin(&self.state.db, self.config.admin.clone()).await?;

        // Send initial enrollment to hub
        if let Some(target) = self.enroll_with {
            let ourself = self.config.issuer(self.role, self.state.key_pair.clone());

            tokio::spawn(
                send_initial_enrollment(target, ourself, self.state.clone()).map_err(|err| {
                    let error = error::chain(&err);
                    error!(%error,"Failed to send initial enrollment");
                }),
            );
        }

        self.router.serve(bind.into()).await?;

        Ok(())
    }
}

/// A server error
#[derive(Debug, Error)]
pub enum Error {
    /// Syncing admin account failed
    #[error("sync admin account")]
    SyncAdmin(#[from] account::Error),
    /// gRPC transport error
    #[error(transparent)]
    Serve(#[from] tonic::transport::Error),
}
