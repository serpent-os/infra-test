use std::{convert::Infallible, net::SocketAddr};

use http::{Request, Response};
use thiserror::Error;
use tonic::{
    body::BoxBody,
    service::interceptor::InterceptorLayer,
    transport::{self, server::Routes, Body, NamedService},
};
use tower::{
    layer::util::{Identity, Stack},
    Layer, Service,
};

use crate::{
    account,
    endpoint::{self, Role},
    middleware, token, Config, State,
};

pub async fn start<T>(
    bind: impl Into<SocketAddr>,
    role: Role,
    config: Config<T>,
    state: State,
) -> Result<(), Error> {
    Server::new(role, config, state).start(bind).await
}

pub type DefaultMiddleware = Stack<
    middleware::Auth,
    Stack<middleware::Log, Stack<InterceptorLayer<middleware::Extensions>, Identity>>,
>;

pub struct Server<T, L> {
    router: transport::server::Router<L>,
    config: Config<T>,
    state: State,
}

impl<T> Server<T, DefaultMiddleware> {
    pub fn new(role: Role, config: Config<T>, state: State) -> Self {
        let endpoint_service = endpoint::Server::new(endpoint::Service {
            issuer: config.issuer(role, state.key_pair.clone()),
        });
        let router = tonic::transport::Server::builder()
            .layer(tonic::service::interceptor(middleware::Extensions {
                db: state.db.clone(),
                pending_enrollment: state.pending_enrollment.clone(),
            }))
            .layer(middleware::Log)
            .layer(middleware::Auth {
                pub_key: state.key_pair.public_key(),
                validation: token::Validation::new().iss(role.service_name()),
            })
            .add_service(endpoint_service);

        Self {
            router,
            config,
            state,
        }
    }
}

impl<T, L> Server<T, L>
where
    L: Layer<Routes>,
    L::Service: Service<Request<Body>, Response = Response<BoxBody>> + Clone + Send + 'static,
    <L::Service as Service<Request<Body>>>::Future: Send + 'static,
    <L::Service as Service<Request<Body>>>::Error:
        Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
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

    pub async fn start(self, bind: impl Into<SocketAddr>) -> Result<(), Error> {
        account::sync_admin(&self.state.db, self.config.admin.clone()).await?;

        self.router.serve(bind).await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("sync admin account: {0}")]
    SyncAdmin(#[from] account::Error),
    #[error(transparent)]
    Serve(#[from] tonic::transport::Error),
}
