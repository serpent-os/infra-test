use service::{api, collectable, database, endpoint, Database, Endpoint};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::worker;

pub fn service(db: Database, worker: worker::Sender) -> api::Service {
    api::Service::new()
        .register::<api::v1::vessel::Build, Error, _>(import_packages)
        .with_state(State { db, worker })
}

#[derive(Clone)]
struct State {
    db: Database,
    worker: worker::Sender,
}

#[tracing::instrument(
    skip_all,
    fields(
        task_id = %request.body.task_id,
        num_collectables = request.body.collectables.len()
    )
)]
async fn import_packages(request: api::Request<api::v1::vessel::Build>, state: State) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;
    let endpoint = Endpoint::get(state.db.acquire().await?.as_mut(), endpoint_id)
        .await
        .map_err(Error::LoadEndpoint)?;

    let body = request.body;

    let packages = body
        .collectables
        .into_iter()
        .filter_map(|c| {
            matches!(c.kind, collectable::Kind::Package).then_some(c.uri.parse().map(|url| worker::Package {
                url,
                sha256sum: c.sha256sum,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if packages.is_empty() {
        warn!(endpoint = %endpoint.id, "No packages to import");
        return Ok(());
    }

    info!(
        endpoint = %endpoint.id,
        num_packages = packages.len(),
        "Import packages"
    );

    state
        .worker
        .send(worker::Message::ImportPackages {
            task_id: body.task_id,
            endpoint,
            packages,
        })
        .map_err(Error::SendWorker)?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    /// Required token is missing from the request
    #[error("Token missing from request")]
    MissingRequestToken,
    /// Endpoint (UUIDv4) cannot be parsed from string
    #[error("invalid endpoint")]
    InvalidEndpoint(#[source] uuid::Error),
    /// Url cannot be parsed from string
    #[error("invalid url")]
    InvalidUrl(#[from] url::ParseError),
    /// Failed to load endpoint from DB
    #[error("load endpoint")]
    LoadEndpoint(#[source] database::Error),
    /// Failed to send task to worker
    #[error("send task to worker")]
    SendWorker(#[source] mpsc::error::SendError<worker::Message>),
    /// Database error
    #[error("database")]
    Database(#[from] database::Error),
}

impl From<&Error> for http::StatusCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::MissingRequestToken => http::StatusCode::UNAUTHORIZED,
            Error::InvalidEndpoint(_) | Error::InvalidUrl(_) => http::StatusCode::BAD_REQUEST,
            Error::LoadEndpoint(_) | Error::SendWorker(_) | Error::Database(_) => {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}
