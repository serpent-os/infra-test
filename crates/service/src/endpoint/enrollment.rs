use chrono::Utc;
use http::Uri;
use log::info;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    proto::{self, EnrollmentRequest, EnrollmentRole},
    service, Role,
};
use crate::{
    account::{self, BearerToken},
    crypto::{EncodedPublicKey, KeyPair, PublicKey},
    database, endpoint,
    sync::SharedMap,
    token::{self, VerifiedToken},
    Account, Database, Endpoint, Token,
};

pub type PendingEnrollment = SharedMap<endpoint::Id, Enrollment>;

#[derive(Debug, Clone)]
pub struct Issuer {
    pub key_pair: KeyPair,
    pub host_address: Uri,
    pub role: Role,
    pub admin_name: String,
    pub admin_email: String,
    pub description: String,
}

impl From<Issuer> for proto::Issuer {
    fn from(issuer: Issuer) -> Self {
        let Issuer {
            key_pair,
            host_address,
            role,
            admin_name,
            admin_email,
            description,
        } = issuer;

        proto::Issuer {
            public_key: key_pair.public_key().encode().to_string(),
            url: host_address.to_string(),
            role: EnrollmentRole::from(role) as i32,
            admin_name,
            admin_email,
            description,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    #[serde(with = "http_serde::uri")]
    pub host_address: Uri,
    pub public_key: PublicKey,
    pub description: String,
    pub admin_email: String,
    pub admin_name: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct Enrollment {
    pub endpoint: endpoint::Id,
    pub account: account::Id,
    pub target: Target,
    pub token: VerifiedToken,
}

impl Enrollment {
    /// Create and send the enrollment
    pub async fn send(target: Target, ourself: Issuer) -> Result<Self, Error> {
        let endpoint = endpoint::Id::generate();
        let account = account::Id::generate();

        let bearer_token = create_bearer_token(endpoint, account, target.role, &ourself)?;

        let enrollment = Enrollment {
            endpoint,
            account,
            target,
            token: bearer_token.clone(),
        };

        let mut client = service::Client::connect(enrollment.target.host_address.clone()).await?;

        let resp = client
            .enroll(EnrollmentRequest {
                issuer: Some(ourself.into()),
                issue_token: bearer_token.encoded,
                role: EnrollmentRole::from(enrollment.target.role) as i32,
            })
            .await;

        match resp {
            Ok(_) => {
                info!("Enrollment request sent for endpoint {endpoint}");

                Ok(enrollment)
            }
            Err(error) => Err(Error::Grpc(error)),
        }
    }

    /// Accept the enrollment from the receiving side
    pub async fn accept(self, db: &Database, ourself: Issuer) -> Result<(), Error> {
        let account_id = self.account;
        let username = format!("@{account_id}");

        Account {
            id: account_id,
            kind: account::Kind::Service,
            username: username.clone(),
            email: self.target.admin_email.clone(),
            public_key: self.target.public_key.encode(),
        }
        .save(&db.pool)
        .await
        .map_err(Error::CreateServiceAccount)?;

        info!("Created a new service account {account_id}");

        let endpoint_id = self.endpoint;
        let extension = match self.target.role {
            Role::Builder => Some(endpoint::Extension::Builder(endpoint::builder::Extension {
                admin_email: self.target.admin_email.clone(),
                admin_name: self.target.admin_name.clone(),
                description: self.target.description.clone(),
                work_status: endpoint::builder::WorkStatus::Idle,
            })),
            Role::RepositoryManager => todo!(),
            Role::Hub => None,
        };

        let mut endpoint = Endpoint {
            id: endpoint_id,
            host_address: self.target.host_address.clone(),
            status: endpoint::Status::AwaitingAcceptance,
            error: None,
            bearer_token: Some(self.token.encoded.clone()),
            api_token: None,
            account: account_id,
            extension,
        };
        endpoint.save(db).await.map_err(Error::CreateEndpoint)?;

        info!("Created a new endpoint {endpoint_id} associated to {username}");

        let bearer_token =
            create_bearer_token(endpoint_id, account_id, self.target.role, &ourself)?;

        BearerToken::set(
            db,
            account_id,
            &bearer_token.encoded,
            bearer_token.expires(),
        )
        .await
        .map_err(Error::SetBearerToken)?;

        info!(
            "Bearer token created for {account_id}, expires on {}",
            bearer_token.expires()
        );

        let mut client = service::connect_with_auth(self.target.host_address, self.token).await?;

        let resp = client
            .accept(EnrollmentRequest {
                issuer: Some(ourself.into()),
                issue_token: bearer_token.encoded,
                role: EnrollmentRole::from(self.target.role) as i32,
            })
            .await;

        match resp {
            Ok(_) => {
                endpoint.status = endpoint::Status::Operational;
                endpoint
                    .save(db)
                    .await
                    .map_err(Error::UpdateEndpointStatus)?;

                info!("Accepted endpoint {endpoint_id}, now operational");

                Ok(())
            }
            Err(error) => {
                endpoint.status = endpoint::Status::Failed;
                endpoint.error = Some(error.message().to_string());
                endpoint
                    .save(db)
                    .await
                    .map_err(Error::UpdateEndpointStatus)?;

                Err(Error::Grpc(error))
            }
        }
    }

    /// Decline the enrollment from the receiving side
    pub async fn decline(self) -> Result<(), Error> {
        let mut client = service::connect_with_auth(self.target.host_address, self.token).await?;

        client.decline(()).await?;

        Ok(())
    }

    /// Mark the enrollment as accepted from the sending side
    pub async fn accepted(
        &self,
        db: &Database,
        public_key: &PublicKey,
        bearer_token: String,
    ) -> Result<(), Error> {
        if *public_key != self.target.public_key {
            return Err(Error::PublicKeyMismatch {
                expected: self.target.public_key.encode(),
                actual: public_key.encode(),
            });
        }

        let account = self.account;
        let username = format!("@{account}");

        Account {
            id: account,
            kind: account::Kind::Service,
            username: username.clone(),
            email: self.target.admin_email.clone(),
            public_key: self.target.public_key.encode(),
        }
        .save(&db.pool)
        .await
        .map_err(Error::CreateServiceAccount)?;

        info!("Created a new service account {account}: {username}");

        let endpoint = self.endpoint;

        Endpoint {
            id: endpoint,
            host_address: self.target.host_address.clone(),
            status: endpoint::Status::Operational,
            error: None,
            bearer_token: Some(bearer_token),
            api_token: None,
            account,
            // Hub endpoint has no extensions
            extension: None,
        }
        .save(db)
        .await
        .map_err(Error::CreateEndpoint)?;

        info!("Created a new endpoint {endpoint} associated to {username}");

        BearerToken::set(db, self.account, &self.token.encoded, self.token.expires())
            .await
            .map_err(Error::SetBearerToken)?;

        info!(
            "Bearer token created for {account}, expires on {}",
            self.token.expires()
        );

        info!("Accepted endpoint {endpoint}, now operational");

        Ok(())
    }
}

fn create_bearer_token(
    endpoint: endpoint::Id,
    account: account::Id,
    role: Role,
    ourself: &Issuer,
) -> Result<VerifiedToken, Error> {
    let purpose = token::Purpose::Authorization;
    let now = Utc::now();
    let expires_on = now + purpose.duration();

    let token = Token::new(token::Payload {
        aud: role.service_name().to_string(),
        exp: expires_on.timestamp(),
        iat: now.timestamp(),
        iss: ourself.role.service_name().to_string(),
        sub: endpoint.to_string(),
        purpose,
        account_id: account,
        account_type: account::Kind::Service,
    });
    let issue_token = token.sign(&ourself.key_pair)?;

    Ok(VerifiedToken {
        encoded: issue_token,
        decoded: token,
    })
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("grpc request failed")]
    Grpc(#[from] tonic::Status),
    #[error("create service account")]
    CreateServiceAccount(#[source] account::Error),
    #[error("create endpoint")]
    CreateEndpoint(#[source] database::Error),
    #[error("set bearer token")]
    SetBearerToken(#[source] account::Error),
    #[error("update endpoint status")]
    UpdateEndpointStatus(#[source] database::Error),
    #[error("public key mismatch, expected {expected} got {actual}")]
    PublicKeyMismatch {
        expected: EncodedPublicKey,
        actual: EncodedPublicKey,
    },
    #[error("grpc transport")]
    Transport(#[from] tonic::transport::Error),
    #[error("sign token")]
    SignToken(#[from] token::Error),
}
