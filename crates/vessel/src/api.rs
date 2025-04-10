use service::{Database, Endpoint, api, collectable, database, endpoint};
use snafu::{ResultExt, Snafu};
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
        .context(InvalidEndpointSnafu)?;
    let endpoint = Endpoint::get(state.db.acquire().await.context(DatabaseSnafu)?.as_mut(), endpoint_id)
        .await
        .context(LoadEndpointSnafu)?;

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
        .collect::<Result<Vec<_>, _>>()
        .context(InvalidUrlSnafu)?;

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
        .context(SendWorkerSnafu)?;

    Ok(())
}

#[derive(Debug, Snafu)]
pub enum Error {
    /// Required token is missing from the request
    #[snafu(display("Token missing from request"))]
    MissingRequestToken,
    /// Endpoint (UUIDv4) cannot be parsed from string
    #[snafu(display("Invalid endpoint"))]
    InvalidEndpoint { source: uuid::Error },
    /// Url cannot be parsed from string
    #[snafu(display("Invalid url"))]
    InvalidUrl { source: url::ParseError },
    /// Failed to load endpoint from DB
    #[snafu(display("Failed to load endpoint"))]
    LoadEndpoint { source: database::Error },
    /// Failed to send task to worker
    #[snafu(display("Failed to send task to worker"))]
    SendWorker {
        source: mpsc::error::SendError<worker::Message>,
    },
    /// Database error
    #[snafu(display("Database error"))]
    Database { source: database::Error },
}

impl From<&Error> for http::StatusCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::MissingRequestToken => http::StatusCode::UNAUTHORIZED,
            Error::InvalidEndpoint { .. } | Error::InvalidUrl { .. } => http::StatusCode::BAD_REQUEST,
            Error::LoadEndpoint { .. } | Error::SendWorker { .. } | Error::Database { .. } => {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}
