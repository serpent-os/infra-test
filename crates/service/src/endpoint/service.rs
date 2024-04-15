//! A [`Service`] implementing the [`EndpointService`] interface

use std::sync::Arc;

use http::Uri;
use thiserror::Error;
use tonic::{
    service::{interceptor::InterceptedService, Interceptor},
    transport::Channel,
};
use tracing::{debug, error, info};

use super::enrollment::{self, Issuer};
use super::proto::{self, EndpointArray, EndpointId, EndpointRole, EnrollmentRequest};
use crate::{
    account,
    crypto::EncodedPublicKey,
    endpoint,
    middleware::{auth, log_handler},
    token::{self, VerifiedToken},
    Database, Role, Token,
};

pub use super::proto::endpoint_service_server::EndpointService;

const ENROLLMENT_FLAGS: auth::Flags = auth::Flags::from_bits_truncate(
    auth::Flags::NOT_EXPIRED.bits()
        | auth::Flags::ACCOUNT_TOKEN.bits()
        | auth::Flags::SERVICE_ACCOUNT.bits(),
);
const ADMIN_FLAGS: auth::Flags = auth::Flags::from_bits_truncate(
    auth::Flags::NOT_EXPIRED.bits()
        | auth::Flags::API_TOKEN.bits()
        | auth::Flags::ADMIN_ACCOUNT.bits(),
);

/// A gRPC server capable of routing [`EndpointService`] requests to be handled by [`Service`]
pub type Server = proto::endpoint_service_server::EndpointServiceServer<Service>;

/// An implementation of the [`EndpointService`] interface
pub struct Service {
    /// Issuer details of this service
    pub issuer: Issuer,
    /// Shared database of this service
    pub db: Database,
    /// Pending enrollment requests that are awaiting confirmation
    pub pending_enrollment: enrollment::Pending,
}

impl Service {
    fn role(&self) -> EndpointRole {
        self.issuer.role.into()
    }

    async fn enroll(&self, request: tonic::Request<EnrollmentRequest>) -> Result<(), Error> {
        let request = request.into_inner();

        // Proto3 makes all message types optional for backwards compatability
        let issuer = request.issuer.as_ref().ok_or(Error::MalformedRequest)?;
        let public_key =
            EncodedPublicKey::decode(&issuer.public_key).map_err(|_| Error::InvalidPublicKey)?;
        let verified_token = Token::verify(
            &request.account_token,
            &public_key,
            &token::Validation::new(),
        )
        .map_err(Error::VerifyToken)?;

        if !matches!(
            verified_token.decoded.payload.purpose,
            token::Purpose::Account
        ) {
            return Err(Error::RequireAccountToken);
        }
        if request.role() != self.role() {
            return Err(Error::RoleMismatch {
                expected: self.role(),
                provided: request.role(),
            });
        }

        info!(
            public_key = issuer.public_key,
            url = issuer.url,
            role = %Role::from(issuer.role()),
            email = issuer.admin_email,
            "Enrollment requested"
        );

        let endpoint = endpoint::Id::generate();
        let account = account::Id::generate();

        debug!(%endpoint, %account, "Generated endpoint & account IDs for enrollment request");

        self.pending_enrollment
            .received
            .insert(
                endpoint,
                enrollment::Received {
                    endpoint,
                    account,
                    remote: enrollment::Remote {
                        host_address: issuer.url.parse::<Uri>()?,
                        public_key,
                        description: issuer.description.clone(),
                        admin_email: issuer.admin_email.clone(),
                        admin_name: issuer.admin_name.clone(),
                        role: issuer.role().into(),
                        token: verified_token,
                    },
                },
            )
            .await;

        Ok(())
    }

    async fn accept(&self, request: tonic::Request<EnrollmentRequest>) -> Result<(), Error> {
        let token = request
            .extensions()
            .get::<VerifiedToken>()
            .cloned()
            .ok_or(Error::MissingRequestToken)?;

        let request = request.into_inner();

        let issuer = request.issuer.as_ref().ok_or(Error::MalformedRequest)?;
        let public_key =
            EncodedPublicKey::decode(&issuer.public_key).map_err(|_| Error::InvalidPublicKey)?;
        let verified_token = Token::verify(
            &request.account_token,
            &public_key,
            &token::Validation::new(),
        )
        .map_err(Error::VerifyToken)?;

        if !matches!(
            verified_token.decoded.payload.purpose,
            token::Purpose::Account
        ) {
            return Err(Error::RequireAccountToken);
        }
        if request.role() != self.role() {
            return Err(Error::RoleMismatch {
                expected: self.role(),
                provided: request.role(),
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
            role = %Role::from(issuer.role()),
            email = issuer.admin_email,
            "Enrollment accepted"
        );

        self.pending_enrollment
            .sent
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?
            .accepted(
                &self.db,
                enrollment::Remote {
                    host_address: issuer.url.parse::<Uri>()?,
                    public_key,
                    description: issuer.description.clone(),
                    admin_email: issuer.admin_email.clone(),
                    admin_name: issuer.admin_name.clone(),
                    role: issuer.role().into(),
                    token: verified_token,
                },
            )
            .await?;

        Ok(())
    }

    async fn decline(&self, request: tonic::Request<()>) -> Result<(), Error> {
        let token = request
            .extensions()
            .get::<VerifiedToken>()
            .cloned()
            .ok_or(Error::MissingRequestToken)?;

        let endpoint = token
            .decoded
            .payload
            .sub
            .parse::<endpoint::Id>()
            .map_err(Error::InvalidEndpoint)?;

        if let Some(enrollment) = self.pending_enrollment.sent.remove(&endpoint).await {
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

    async fn pending(&self, _request: tonic::Request<()>) -> Result<EndpointArray, Error> {
        let endpoints = self
            .pending_enrollment
            .received
            .all()
            .await
            .into_values()
            .map(|enrollment| proto::Endpoint {
                id: Some(proto::EndpointId {
                    id: enrollment.endpoint.to_string(),
                }),
                host_address: enrollment.remote.host_address.to_string(),
                public_key: enrollment.remote.public_key.encode().to_string(),
                status: proto::EndpointStatus::AwaitingAcceptance as i32,
            })
            .collect();

        Ok(EndpointArray { endpoints })
    }

    async fn accept_pending(&self, request: tonic::Request<EndpointId>) -> Result<(), Error> {
        let request = request.into_inner();
        let endpoint = request
            .id
            .parse::<endpoint::Id>()
            .map_err(Error::InvalidEndpoint)?;

        let enrollment = self
            .pending_enrollment
            .received
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?;

        info!(
            %endpoint,
            public_key = %enrollment.remote.public_key,
            url = %enrollment.remote.host_address,
            role = %enrollment.remote.role,
            email = enrollment.remote.admin_email,
            "Pending enrollment accepted"
        );

        enrollment
            .accept(&self.db, self.issuer.clone())
            .await
            .map_err(Error::Enrollment)?;

        Ok(())
    }

    async fn decline_pending(&self, request: tonic::Request<EndpointId>) -> Result<(), Error> {
        let request = request.into_inner();
        let endpoint = request
            .id
            .parse::<endpoint::Id>()
            .map_err(Error::InvalidEndpoint)?;

        let enrollment = self
            .pending_enrollment
            .received
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?;

        info!(
            %endpoint,
            public_key = %enrollment.remote.public_key,
            url = %enrollment.remote.host_address,
            role = %enrollment.remote.role,
            email = enrollment.remote.admin_email,
            "Pending enrollment declined"
        );

        enrollment.decline().await.map_err(Error::Enrollment)?;

        Ok(())
    }
}

#[tonic::async_trait]
impl EndpointService for Service {
    #[tracing::instrument(skip_all)]
    async fn enroll(
        &self,
        request: tonic::Request<EnrollmentRequest>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        // Technically the same as ommitting this check
        auth(&request, auth::Flags::NO_AUTH)?;

        if matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.enroll(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn accept(
        &self,
        request: tonic::Request<EnrollmentRequest>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, ENROLLMENT_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.accept(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn decline(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, ENROLLMENT_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.decline(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn leave(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, ENROLLMENT_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            Err(tonic::Status::unimplemented("unimplemented"))
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn pending(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<EndpointArray>, tonic::Status> {
        auth(&request, ADMIN_FLAGS)?;

        if matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.pending(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn accept_pending(
        &self,
        request: tonic::Request<EndpointId>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, ADMIN_FLAGS)?;

        if matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.accept_pending(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    #[tracing::instrument(skip_all)]
    async fn decline_pending(
        &self,
        request: tonic::Request<EndpointId>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, ADMIN_FLAGS)?;

        if matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.decline_pending(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }
}

/// A client that can connect to and call the [`EndpointService`] interface
pub type Client<T> = proto::endpoint_service_client::EndpointServiceClient<T>;

/// Connect to the [`EndpointService`] at [`Uri`] with authorization via the provided API `token`
pub async fn connect_with_auth(
    uri: Uri,
    token: String,
) -> Result<Client<InterceptedService<Channel, impl Interceptor>>, tonic::transport::Error> {
    let channel = Channel::builder(uri).connect().await?;
    Ok(Client::with_interceptor(
        channel,
        move |mut req: tonic::Request<()>| {
            let auth_header = format!("Bearer {}", token)
                .parse()
                .map_err(|_| tonic::Status::internal(""))?;
            req.metadata_mut().insert("authorization", auth_header);
            Ok(req)
        },
    ))
}

/// An error when handling an [`EndpointService`] request
#[derive(Debug, Error)]
pub enum Error {
    /// Required token is missing from the request
    #[error("Token missing from request")]
    MissingRequestToken,
    /// Request is malformed
    #[error("Malformed request")]
    MalformedRequest,
    /// Request requires an account token
    #[error("Requires an account token")]
    RequireAccountToken,
    /// Public key is invalid and can't be decoded
    #[error("Invalid public key")]
    InvalidPublicKey,
    /// Role on request doesn't match role of service
    #[error("Role mismatch, expected {expected:?} provided {provided:?}")]
    RoleMismatch {
        /// The expected role
        expected: EndpointRole,
        /// The provided role
        provided: EndpointRole,
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
    /// Token verfication failed
    #[error("verify token")]
    VerifyToken(#[source] token::Error),
    /// An enrollment error
    #[error("enrollment")]
    Enrollment(#[from] enrollment::Error),
}

impl From<Error> for tonic::Status {
    fn from(error: Error) -> Self {
        let mut status = match &error {
            Error::MissingRequestToken => tonic::Status::internal(""),
            Error::MalformedRequest => tonic::Status::internal(""),
            Error::VerifyToken(_) => tonic::Status::internal(""),
            Error::Enrollment(_) => tonic::Status::internal(""),
            Error::InvalidPublicKey => tonic::Status::invalid_argument("not a valid public key"),
            Error::InvalidUrl(_) => tonic::Status::invalid_argument("not a valid URL"),
            Error::InvalidEndpoint(_) => tonic::Status::invalid_argument(""),
            Error::RequireAccountToken => tonic::Status::invalid_argument("account token required"),
            Error::RoleMismatch { expected, .. } => {
                tonic::Status::invalid_argument(format!("only supported by {expected:?} role"))
            }
            Error::MissingPendingEnrollment(_) => tonic::Status::invalid_argument(
                "open enrollment doesn't exist for request, must re-enroll",
            ),
        };
        status.set_source(Arc::new(error));
        status
    }
}
