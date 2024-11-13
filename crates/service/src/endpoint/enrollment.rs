//! Enroll with remote services to provision authorization

use http::Uri;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::{
    account, api, client,
    crypto::{EncodedPublicKey, KeyPair, PublicKey},
    database, endpoint,
    token::{self, VerifiedToken},
    Account, Client, Database, Endpoint, Role,
};

/// An issuer of enrollment requests
#[derive(Debug, Clone)]
pub struct Issuer {
    /// [`KeyPair`] for creating / validating tokens
    pub key_pair: KeyPair,
    /// [`Uri`] the issuer can be reached at
    pub host_address: Uri,
    /// Endpoint role
    pub role: Role,
    /// Endpoint description
    pub description: String,
    /// Admin name
    pub admin_name: String,
    /// Admin email
    pub admin_email: String,
}

impl From<Issuer> for service_types::endpoint::enrollment::Issuer {
    fn from(issuer: Issuer) -> Self {
        let Issuer {
            key_pair,
            host_address,
            role,
            ..
        } = issuer;

        service_types::endpoint::enrollment::Issuer {
            public_key: key_pair.public_key().encode().to_string(),
            url: host_address.to_string(),
            role,
        }
    }
}

/// The remote details of an enrollment request
#[derive(Debug, Clone)]
pub struct Remote {
    /// [`PublicKey`] of the remote endpoint
    pub public_key: PublicKey,
    /// [`Uri`] the remote endpoint can be reached at
    pub host_address: Uri,
    /// Remote endpoint role
    pub role: Role,
    /// Bearer token assigned to us by the remote endpoint
    pub bearer_token: VerifiedToken,
}

/// A received enrollment request
#[derive(Debug, Clone)]
pub struct Received {
    /// UUID to assign the endpoint of this request
    pub endpoint: endpoint::Id,
    /// UUID to assign the service account of this request
    pub account: account::Id,
    /// Remote details of the enrollment request
    pub remote: Remote,
}

/// A sent enrollment request
#[derive(Debug, Clone)]
pub struct Sent {
    /// UUID to assign the endpoint of this request
    pub endpoint: endpoint::Id,
    /// UUID to assign the service account of this request
    pub account: account::Id,
    /// Target of the enrollment request
    pub target: Target,
    /// Bearer token we've issued and sent along w/ the request
    pub bearer_token: VerifiedToken,
}

/// The target of a [`Sent`] enrollment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// [`Uri`] the target endpoint can be reached at
    #[serde(with = "http_serde::uri")]
    pub host_address: Uri,
    /// [`PublicKey`] of the target endpoint
    pub public_key: PublicKey,
    /// Target endpoint role
    pub role: Role,
}

#[tracing::instrument(
    name = "send_enrollment", 
    skip_all,
    fields(
        public_key = %target.public_key,
        url = %target.host_address,
        role = %target.role,
    )
)]
/// Create and send an enrollment request to [`Target`]
pub async fn send(target: Target, ourself: Issuer) -> Result<Sent, Error> {
    let endpoint = endpoint::Id::generate();
    let account = account::Id::generate();

    debug!(%endpoint, %account, "Generated endpoint & account IDs for enrollment request");

    let bearer_token = endpoint::create_token(token::Purpose::Authorization, endpoint, account, target.role, &ourself)?;

    let client = Client::new(target.host_address.clone());

    let resp = client
        .send::<api::v1::services::Enroll>(&api::v1::services::EnrollRequestBody {
            request: service_types::endpoint::enrollment::Request {
                issuer: ourself.into(),
                issue_token: bearer_token.encoded.clone(),
                role: target.role,
            },
        })
        .await;

    match resp {
        Ok(_) => {
            info!(
                %endpoint,
                %account,
                public_key = %target.public_key,
                url = %target.host_address,
                role = %target.role,
                "Enrollment request sent"
            );

            Ok(Sent {
                endpoint,
                account,
                target,
                bearer_token,
            })
        }
        Err(error) => Err(Error::Client(error)),
    }
}

impl Received {
    /// Accept the received enrollment
    #[tracing::instrument(
        name = "accept_enrollment",
        skip_all,
        fields(
            endpoint = %self.endpoint,
            account = %self.account,
            public_key = %self.remote.public_key,
            url = %self.remote.host_address,
            role = %self.remote.role,
        )
    )]
    pub async fn accept(self, db: &Database, ourself: Issuer) -> Result<(), Error> {
        let account_id = self.account;
        let username = format!("@{account_id}");

        Account::service(account_id, self.remote.public_key.encode())
            .save(&db.pool)
            .await
            .map_err(Error::CreateServiceAccount)?;

        info!(username, "Created a new service account");

        let endpoint_id = self.endpoint;
        let kind = match self.remote.role {
            Role::Builder => endpoint::Kind::Builder(endpoint::builder::Extension {
                work_status: endpoint::builder::WorkStatus::Idle,
            }),
            Role::RepositoryManager => endpoint::Kind::RepositoryManager,
            Role::Hub => endpoint::Kind::Hub,
        };

        let mut endpoint = Endpoint {
            id: endpoint_id,
            host_address: self.remote.host_address.clone(),
            status: endpoint::Status::AwaitingAcceptance,
            error: None,
            account: account_id,
            kind,
        };
        endpoint.save(db).await.map_err(Error::CreateEndpoint)?;

        endpoint::Tokens {
            bearer_token: Some(self.remote.bearer_token.encoded.clone()),
            access_token: None,
        }
        .save(db, endpoint.id)
        .await
        .map_err(Error::SetEndpointAccountToken)?;

        info!("Created a new endpoint for the service account");

        let bearer_token = endpoint::create_token(
            token::Purpose::Authorization,
            endpoint_id,
            account_id,
            self.remote.role,
            &ourself,
        )?;

        account::Token::set(db, account_id, &bearer_token.encoded, bearer_token.expires())
            .await
            .map_err(Error::SetAccountToken)?;

        info!(
            expiration = %bearer_token.expires(),
            "Account token created",
        );

        let resp = Client::new(self.remote.host_address)
            .with_tokens(client::Tokens {
                bearer_token: Some(self.remote.bearer_token.clone()),
                access_token: None,
            })
            .send::<api::v1::services::Accept>(&api::v1::services::AcceptRequestBody {
                request: service_types::endpoint::enrollment::Request {
                    issuer: ourself.into(),
                    issue_token: bearer_token.encoded,
                    role: self.remote.role,
                },
            })
            .await;

        match resp {
            Ok(_) => {
                endpoint.status = endpoint::Status::Operational;
                endpoint.save(db).await.map_err(Error::UpdateEndpointStatus)?;

                info!("Accepted endpoint now operational");

                Ok(())
            }
            Err(error) => {
                endpoint.status = endpoint::Status::Failed;
                endpoint.error = Some(error.to_string());
                endpoint.save(db).await.map_err(Error::UpdateEndpointStatus)?;

                Err(Error::Client(error))
            }
        }
    }

    /// Decline the received enrollment
    pub async fn decline(self) -> Result<(), Error> {
        Client::new(self.remote.host_address)
            .with_tokens(client::Tokens {
                bearer_token: Some(self.remote.bearer_token.clone()),
                access_token: None,
            })
            .send::<api::v1::services::Decline>(&())
            .await?;

        Ok(())
    }
}

impl Sent {
    /// Mark the sent enrollment as accepted
    #[tracing::instrument(
        name = "accepted_enrollment",
        skip_all,
        fields(
            endpoint = %self.endpoint,
            account = %self.account,
            public_key = %self.target.public_key,
            url = %self.target.host_address,
            role = %self.target.role,
        )
    )]
    pub async fn accepted(&self, db: &Database, remote: Remote) -> Result<(), Error> {
        if remote.public_key != self.target.public_key {
            return Err(Error::PublicKeyMismatch {
                expected: self.target.public_key.encode(),
                actual: remote.public_key.encode(),
            });
        }

        let account = self.account;
        let username = format!("@{account}");

        Account {
            id: account,
            kind: account::Kind::Service,
            username: username.clone(),
            email: None,
            name: None,
            public_key: self.target.public_key.encode(),
        }
        .save(&db.pool)
        .await
        .map_err(Error::CreateServiceAccount)?;

        info!(username, "Created a new service account");

        let endpoint = self.endpoint;

        Endpoint {
            id: endpoint,
            host_address: self.target.host_address.clone(),
            status: endpoint::Status::Operational,
            error: None,
            account,
            kind: endpoint::Kind::Hub,
        }
        .save(db)
        .await
        .map_err(Error::CreateEndpoint)?;

        endpoint::Tokens {
            bearer_token: Some(remote.bearer_token.encoded),
            access_token: None,
        }
        .save(db, endpoint)
        .await
        .map_err(Error::SetEndpointAccountToken)?;

        info!("Created a new endpoint for the service account");

        account::Token::set(
            db,
            self.account,
            &self.bearer_token.encoded,
            self.bearer_token.expires(),
        )
        .await
        .map_err(Error::SetAccountToken)?;

        info!(
            expiration = %self.bearer_token.expires(),
            "Account token saved",
        );

        info!("Accepted endpoint now operational");

        Ok(())
    }
}

/// An enrollment error
#[derive(Debug, Error)]
pub enum Error {
    /// Reading an [`Account`] failed
    #[error("read account")]
    ReadAccount(#[source] account::Error),
    /// Creating a service [`Account`] failed
    #[error("create service account")]
    CreateServiceAccount(#[source] account::Error),
    /// Listing endpoints failed
    #[error("list endpoints")]
    ListEndpoints(#[source] database::Error),
    /// Creating an [`Endpoint`] failed
    #[error("create endpoint")]
    CreateEndpoint(#[source] database::Error),
    /// Setting the account token given by an endpoint failed
    #[error("set endpoint account token")]
    SetEndpointAccountToken(#[source] database::Error),
    /// Setting the account token given to an endpoint failed
    #[error("set account token")]
    SetAccountToken(#[source] account::Error),
    /// Updating the endpoint status failed
    #[error("update endpoint status")]
    UpdateEndpointStatus(#[source] database::Error),
    /// Public key doesn't match expected value
    #[error("public key mismatch, expected {expected} got {actual}")]
    PublicKeyMismatch {
        /// The expected key
        expected: EncodedPublicKey,
        /// The actual key
        actual: EncodedPublicKey,
    },
    /// Token signing failed
    #[error("sign token")]
    SignToken(#[from] token::Error),
    /// Client error
    #[error("client")]
    Client(#[from] client::Error),
}
