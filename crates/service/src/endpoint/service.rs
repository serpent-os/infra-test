use std::sync::Arc;

use http::Uri;
use log::{error, info};
use thiserror::Error;
use tonic::{
    service::{interceptor::InterceptedService, Interceptor},
    transport::Channel,
};

use super::enrollment::{self, Issuer};
use super::proto::{
    self, endpoint_service_server::EndpointService, EndpointArray, EndpointId, EndpointRole,
    EnrollmentRequest,
};
use crate::{
    account,
    crypto::EncodedPublicKey,
    endpoint::{self, enrollment::PendingEnrollment, Enrollment},
    middleware::{auth, log_handler},
    token::{self, VerifiedToken},
    Database, Token,
};

const SERVICE_FLAGS: auth::Flags = auth::Flags::from_bits_truncate(
    auth::Flags::NOT_EXPIRED.bits()
        | auth::Flags::ACCOUNT_TOKEN.bits()
        | auth::Flags::SERVICE_ACCOUNT.bits(),
);
const ADMIN_FLAGS: auth::Flags = auth::Flags::from_bits_truncate(
    auth::Flags::NOT_EXPIRED.bits()
        | auth::Flags::API_TOKEN.bits()
        | auth::Flags::ADMIN_ACCOUNT.bits(),
);

pub type Server = proto::endpoint_service_server::EndpointServiceServer<Service>;

pub struct Service {
    pub issuer: Issuer,
    pub db: Database,
    pub pending_enrollment: PendingEnrollment,
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

        info!("Got an enrollment request: {request:?}");

        let endpoint = endpoint::Id::generate();
        let account = account::Id::generate();

        self.pending_enrollment
            .insert(
                endpoint,
                Enrollment {
                    endpoint,
                    account,
                    target: enrollment::Target {
                        host_address: issuer.url.parse::<Uri>()?,
                        public_key,
                        description: issuer.description.clone(),
                        admin_email: issuer.admin_email.clone(),
                        admin_name: issuer.admin_name.clone(),
                        role: issuer.role().into(),
                    },
                    token: verified_token,
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

        info!("Got an enrollment acceptance for endpoint {endpoint}: {request:?}");

        self.pending_enrollment
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?
            .accepted(&self.db, &public_key, verified_token.encoded)
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

        info!("Endpoint enrollment declined for {endpoint}");

        self.pending_enrollment.remove(&endpoint).await;

        Ok(())
    }

    async fn pending(&self, _request: tonic::Request<()>) -> Result<EndpointArray, Error> {
        let endpoints = self
            .pending_enrollment
            .all()
            .await
            .into_values()
            .map(|enrollment| proto::Endpoint {
                id: Some(proto::EndpointId {
                    id: enrollment.endpoint.to_string(),
                }),
                host_address: enrollment.target.host_address.to_string(),
                public_key: enrollment.target.public_key.encode().to_string(),
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
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?;

        info!("Accepting pending enrollment for endpoint {endpoint}");

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
            .remove(&endpoint)
            .await
            .ok_or(Error::MissingPendingEnrollment(endpoint))?;

        info!("Declining pending enrollment for endpoint {endpoint}");

        enrollment.decline().await.map_err(Error::Enrollment)?;

        Ok(())
    }
}

#[tonic::async_trait]
impl EndpointService for Service {
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

    async fn accept(
        &self,
        request: tonic::Request<EnrollmentRequest>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, SERVICE_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.accept(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }
    async fn decline(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, SERVICE_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            log_handler(self.decline(request).await)
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }
    async fn leave(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        auth(&request, SERVICE_FLAGS)?;

        if !matches!(self.role(), EndpointRole::Hub) {
            Err(tonic::Status::unimplemented("unimplemented"))
        } else {
            Err(tonic::Status::unimplemented(format!(
                "not supported by {:?} role",
                self.role()
            )))
        }
    }

    async fn visible(
        &self,
        request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<EndpointArray>, tonic::Status> {
        auth(&request, SERVICE_FLAGS)?;

        Err(tonic::Status::unimplemented("unimplemented"))
    }

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

pub type Client<T> = proto::endpoint_service_client::EndpointServiceClient<T>;

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

#[derive(Debug, Error)]
pub enum Error {
    #[error("Token missing from request")]
    MissingRequestToken,
    #[error("Malformed request")]
    MalformedRequest,
    #[error("Requires an account token")]
    RequireAccountToken,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Role mismatch, expected {expected:?} provided {provided:?}")]
    RoleMismatch {
        expected: EndpointRole,
        provided: EndpointRole,
    },
    #[error("Pending enrollment missing for endpoint {0}")]
    MissingPendingEnrollment(endpoint::Id),
    #[error("invalid uri")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    #[error("invalid endpoint")]
    InvalidEndpoint(#[source] uuid::Error),
    #[error("verify token")]
    VerifyToken(#[source] token::Error),
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
