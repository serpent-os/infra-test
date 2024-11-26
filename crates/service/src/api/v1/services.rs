//! An implementation of endpoint service operations

use std::time::Duration;

use http::Uri;
use thiserror::Error;
use tracing::{debug, error, info};

pub use service_core::api::v1::services::*;

use crate::{
    account, api,
    crypto::{EncodedPublicKey, PublicKey},
    endpoint::{
        self,
        enrollment::{self, Issuer},
    },
    error,
    sync::SharedMap,
    token, Config, Database, Role, Token,
};

/// An implementation of the shared service operations
//
// Provided by shared [`Server`](crate::Server)
// so doesn't need to be public
pub(crate) fn services(role: Role, config: &Config, state: &crate::State) -> api::Service {
    api::Service::new()
        .register::<Enroll, Error, _>(enroll)
        .register::<Accept, Error, _>(accept)
        .register::<Decline, Error, _>(decline)
        .register::<RefreshToken, Error, _>(refresh_token)
        .register::<RefreshIssueToken, Error, _>(refresh_issue_token)
        .with_state(State {
            issuer: config.issuer(role, state.key_pair.clone()),
            db: state.service_db.clone(),
            pending_sent: state.pending_sent.clone(),
            upstream: config.upstream,
        })
}

/// State for endpoint handlers
#[derive(Debug, Clone)]
struct State {
    /// Issuer details of this service
    issuer: Issuer,
    /// Shared database of this service
    db: Database,
    /// Pending enrollment requests that are awaiting confirmation
    ///
    /// Only applicable for hub service
    pending_sent: SharedMap<endpoint::Id, enrollment::Sent>,
    /// Upstream hub to auto-accept enrollment with
    ///
    /// Only applicable for non-hub services
    upstream: Option<PublicKey>,
}

impl State {
    fn role(&self) -> Role {
        self.issuer.role
    }
}

async fn enroll(request: api::Request<Enroll>, state: State) -> Result<(), Error> {
    let upstream = *state.upstream.as_ref().ok_or(Error::UpstreamNotSet)?;

    let request = request.body.request;
    let issuer = request.issuer;
    let issue_token = request.issue_token;

    let public_key = EncodedPublicKey::decode(&issuer.public_key).map_err(|_| Error::InvalidPublicKey)?;

    if public_key != upstream {
        return Err(Error::UpstreamMismatch {
            expected: upstream,
            provided: public_key,
        });
    }

    let verified_token =
        Token::verify(&issue_token, &public_key, &token::Validation::new()).map_err(Error::VerifyToken)?;

    if !matches!(verified_token.decoded.payload.purpose, token::Purpose::Authorization) {
        return Err(Error::RequireBearerToken);
    }
    if request.role != state.role() {
        return Err(Error::RoleMismatch {
            expected: state.role(),
            provided: request.role,
        });
    }

    info!(
        public_key = issuer.public_key,
        url = issuer.url,
        role = %issuer.role,
        "Enrollment requested"
    );

    let endpoint = endpoint::Id::generate();
    let account = account::Id::generate();

    debug!(%endpoint, %account, "Generated endpoint & account IDs for enrollment request");

    let recieved = enrollment::Received {
        endpoint,
        account,
        remote: enrollment::Remote {
            host_address: issuer.url.parse::<Uri>()?,
            public_key,
            role: issuer.role,
            bearer_token: verified_token,
        },
    };

    // Return from handler and accept in background
    //
    // D infra expects this operation returns before we
    // respond w/ acceptance
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;

        if let Err(e) = recieved.accept(&state.db, state.issuer.clone()).await {
            error!(error=%error::chain(e), "Auto accept failed")
        };
    });

    Ok(())
}

async fn accept(request: api::Request<Accept>, state: State) -> Result<(), Error> {
    let token = request.token.clone().ok_or(Error::MissingRequestToken)?;

    let request = request.body.request;
    let issuer = request.issuer;

    let public_key = EncodedPublicKey::decode(&issuer.public_key).map_err(|_| Error::InvalidPublicKey)?;
    let verified_token =
        Token::verify(&request.issue_token, &public_key, &token::Validation::new()).map_err(Error::VerifyToken)?;

    if !matches!(verified_token.decoded.payload.purpose, token::Purpose::Authorization) {
        return Err(Error::RequireBearerToken);
    }
    if request.role != state.role() {
        return Err(Error::RoleMismatch {
            expected: state.role(),
            provided: request.role,
        });
    }

    let endpoint = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;

    info!(
        %endpoint,
        public_key = issuer.public_key,
        url = issuer.url,
        role = %issuer.role,
        "Enrollment accepted"
    );

    state
        .pending_sent
        .remove(&endpoint)
        .await
        .ok_or(Error::MissingPendingEnrollment(endpoint))?
        .accepted(
            &state.db,
            enrollment::Remote {
                host_address: issuer.url.parse::<Uri>()?,
                public_key,
                role: issuer.role,
                bearer_token: verified_token,
            },
        )
        .await?;

    Ok(())
}

async fn decline(request: api::Request<Decline>, state: State) -> Result<(), Error> {
    let token = request.token.clone().ok_or(Error::MissingRequestToken)?;

    let endpoint = token
        .decoded
        .payload
        .sub
        .parse::<endpoint::Id>()
        .map_err(Error::InvalidEndpoint)?;

    if let Some(enrollment) = state.pending_sent.remove(&endpoint).await {
        info!(
            %endpoint,
            public_key = %enrollment.target.public_key,
            url = %enrollment.target.host_address,
            role = %enrollment.target.role,
            "Enrollment declined"
        );
    }

    Ok(())
}

// Middleware already validates this token is valid for this endpoint
async fn refresh_token(request: api::Request<RefreshToken>, state: State) -> Result<String, Error> {
    request
        .token
        .ok_or(Error::MissingRequestToken)?
        .decoded
        // Bearer token is provided, so make sure
        // we return an access token
        .with_purpose(token::Purpose::Authentication)
        .refresh()
        .sign(&state.issuer.key_pair)
        .map_err(Error::SignToken)
}

// Middleware already validates this token is valid for this endpoint
async fn refresh_issue_token(request: api::Request<RefreshIssueToken>, state: State) -> Result<String, Error> {
    request
        .token
        .ok_or(Error::MissingRequestToken)?
        .decoded
        .refresh()
        .sign(&state.issuer.key_pair)
        .map_err(Error::SignToken)
}

/// An error when handling an [`EndpointService`] request
#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
enum Error {
    /// Required token is missing from the request
    #[error("Token missing from request")]
    MissingRequestToken,
    /// Request requires a bearer token
    #[error("Requires a bearer token")]
    RequireBearerToken,
    /// Public key is invalid and can't be decoded
    #[error("Invalid public key")]
    InvalidPublicKey,
    /// Upstream public key not set
    #[error("Upstream public key not set for auto-enrollment")]
    UpstreamNotSet,
    /// Upstream request came from a different public key
    #[error("Upstream public key mismatch, expected: {expected} provided {provided}")]
    UpstreamMismatch {
        /// The expected public key
        expected: PublicKey,
        /// The provided public key
        provided: PublicKey,
    },
    /// Role on request doesn't match role of service
    #[error("Role mismatch, expected {expected:?} provided {provided:?}")]
    RoleMismatch {
        /// The expected role
        expected: Role,
        /// The provided role
        provided: Role,
    },
    /// No pending enrollment is found for the provided endpoint ID
    #[error("Pending enrollment missing for endpoint {0}")]
    MissingPendingEnrollment(endpoint::Id),
    /// Url cannot be parsed from string
    #[error("invalid uri")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    /// Endpoint (UUIDv4) cannot be parsed from string
    #[error("invalid endpoint")]
    InvalidEndpoint(#[source] uuid::Error),
    /// Token verification failed
    #[error("verify token")]
    VerifyToken(#[source] token::Error),
    #[error("sign token")]
    SignToken(#[source] token::Error),
    /// An enrollment error
    #[error("enrollment")]
    Enrollment(#[from] enrollment::Error),
}

impl From<&Error> for http::StatusCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::MissingRequestToken => http::StatusCode::UNAUTHORIZED,
            Error::Enrollment(_) | Error::UpstreamNotSet | Error::SignToken(_) => {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }
            Error::InvalidPublicKey
            | Error::InvalidUrl(_)
            | Error::InvalidEndpoint(_)
            | Error::RequireBearerToken
            | Error::VerifyToken(_)
            | Error::RoleMismatch { .. }
            | Error::MissingPendingEnrollment(_)
            | Error::UpstreamMismatch { .. } => http::StatusCode::BAD_REQUEST,
        }
    }
}
