use service::{Endpoint, State, api, database, endpoint};
use thiserror::Error;
use tracing::{error, info, warn};

use crate::worker;

pub fn service(state: State, sender: worker::Sender) -> api::Service {
    api::Service::new()
        .register::<api::v1::summit::BuildSucceeded, Error, _>(build_succeeded)
        .register::<api::v1::summit::BuildFailed, Error, _>(build_failed)
        .register::<api::v1::summit::ImportSucceeded, Error, _>(import_succeeded)
        .register::<api::v1::summit::ImportFailed, Error, _>(import_failed)
        .with_state(Context { state, sender })
}

#[derive(Clone)]
struct Context {
    state: State,
    sender: worker::Sender,
}

#[tracing::instrument(
    skip_all,
    fields(
        task_id = %request.body.task_id,
    )
)]
async fn build_succeeded(
    request: api::Request<api::v1::summit::BuildSucceeded>,
    context: Context,
) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;
    let endpoint = Endpoint::get(context.state.service_db.acquire().await?.as_mut(), endpoint_id)
        .await
        .map_err(Error::LoadEndpoint)?;

    info!(
        endpoint = %endpoint.id,
        "Build succeeded"
    );

    let build = request.body;

    let _ = context.sender.send(worker::Message::BuildSucceeded {
        task_id: (build.task_id as i64).into(),
        builder: endpoint.id,
        collectables: build.collectables,
    });

    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(
        task_id = %request.body.task_id,
    )
)]
async fn build_failed(request: api::Request<api::v1::summit::BuildFailed>, context: Context) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;
    let endpoint = Endpoint::get(context.state.service_db.acquire().await?.as_mut(), endpoint_id)
        .await
        .map_err(Error::LoadEndpoint)?;

    warn!(
        endpoint = %endpoint.id,
        "Build failed"
    );

    let build = request.body;

    let _ = context.sender.send(worker::Message::BuildFailed {
        task_id: (build.task_id as i64).into(),
        builder: endpoint.id,
        collectables: build.collectables,
    });

    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(
        task_id = %request.body.task_id,
    )
)]
async fn import_succeeded(
    request: api::Request<api::v1::summit::ImportSucceeded>,
    context: Context,
) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;
    let endpoint = Endpoint::get(context.state.service_db.acquire().await?.as_mut(), endpoint_id)
        .await
        .map_err(Error::LoadEndpoint)?;

    info!(
        endpoint = %endpoint.id,
        "Import succeeded"
    );

    let _ = context.sender.send(worker::Message::ImportSucceeded {
        task_id: (request.body.task_id as i64).into(),
    });

    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(
        task_id = %request.body.task_id,
    )
)]
async fn import_failed(request: api::Request<api::v1::summit::ImportFailed>, context: Context) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;
    let endpoint = Endpoint::get(context.state.service_db.acquire().await?.as_mut(), endpoint_id)
        .await
        .map_err(Error::LoadEndpoint)?;

    warn!(
        endpoint = %endpoint.id,
        "Import failed"
    );

    let _ = context.sender.send(worker::Message::ImportFailed {
        task_id: (request.body.task_id as i64).into(),
    });

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    /// Required token is missing from the request
    #[error("Token missing from request")]
    MissingRequestToken,
    /// Remotes missing from request
    #[error("Missing remotes")]
    MissingRemotes,
    /// Another build is already in progress
    #[error("Another build is already in progress")]
    BuildInProgress,
    /// Endpoint (UUIDv4) cannot be parsed from string
    #[error("invalid endpoint")]
    InvalidEndpoint(#[source] uuid::Error),
    /// Failed to load endpoint from DB
    #[error("load endpoint")]
    LoadEndpoint(#[source] database::Error),
    /// Database error
    #[error("database")]
    Database(#[from] database::Error),
}

impl From<&Error> for http::StatusCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::MissingRequestToken => http::StatusCode::UNAUTHORIZED,
            Error::MissingRemotes | Error::InvalidEndpoint(_) => http::StatusCode::BAD_REQUEST,
            Error::LoadEndpoint(_) | Error::Database(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
            Error::BuildInProgress => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}
