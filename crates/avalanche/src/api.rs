use std::sync::atomic::{self, AtomicBool};

use service::{Endpoint, State, api, database, endpoint};
use snafu::{ResultExt, Snafu, ensure};
use tracing::info;

use crate::Config;

static BUILD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn service(state: State, config: Config) -> api::Service {
    api::Service::new()
        .register::<api::v1::avalanche::Build, Error, _>(build)
        .with_state(Context { state, config })
}

#[derive(Clone)]
struct Context {
    state: State,
    config: Config,
}

#[tracing::instrument(
    skip_all,
    fields(
        build_id = %request.body.request.build_id,
    )
)]
async fn build(request: api::Request<api::v1::avalanche::Build>, context: Context) -> Result<(), Error> {
    let token = request.token.ok_or(Error::MissingRequestToken)?;

    let endpoint_id = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .context(InvalidEndpointSnafu)?;
    let endpoint = Endpoint::get(
        context
            .state
            .service_db
            .acquire()
            .await
            .context(DatabaseSnafu)?
            .as_mut(),
        endpoint_id,
    )
    .await
    .context(LoadEndpointSnafu)?;

    let build = request.body.request;

    ensure!(!build.remotes.is_empty(), MissingRemotesSnafu);

    info!(
        endpoint = %endpoint.id,
        "Build request received"
    );

    // Atomically guarantee another build isn't in progress
    if BUILD_IN_PROGRESS
        .compare_exchange(false, true, atomic::Ordering::SeqCst, atomic::Ordering::Relaxed)
        .is_err()
    {
        return Err(Error::BuildInProgress);
    }

    // Build time!
    tokio::spawn(async move {
        crate::build(build, endpoint, context.state, context.config).await;
        BUILD_IN_PROGRESS.store(false, atomic::Ordering::Relaxed);
    });

    Ok(())
}

#[derive(Debug, Snafu)]
pub enum Error {
    /// Required token is missing from the request
    #[snafu(display("Token missing from request"))]
    MissingRequestToken,
    /// Remotes missing from request
    #[snafu(display("Missing remotes"))]
    MissingRemotes,
    /// Another build is already in progress
    #[snafu(display("Another build is already in progress"))]
    BuildInProgress,
    /// Endpoint (UUIDv4) cannot be parsed from string
    #[snafu(display("Invalid endpoint"))]
    InvalidEndpoint { source: uuid::Error },
    /// Failed to load endpoint from DB
    #[snafu(display("Failed to load endpoint"))]
    LoadEndpoint { source: database::Error },
    /// Database error
    #[snafu(display("Database error"))]
    Database { source: database::Error },
}

impl From<&Error> for http::StatusCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::MissingRequestToken => http::StatusCode::UNAUTHORIZED,
            Error::MissingRemotes | Error::InvalidEndpoint { .. } => http::StatusCode::BAD_REQUEST,
            Error::LoadEndpoint { .. } | Error::Database { .. } => http::StatusCode::INTERNAL_SERVER_ERROR,
            Error::BuildInProgress => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}
