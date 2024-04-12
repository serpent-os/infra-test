use chrono::Utc;
use http::Uri;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use super::{
    proto::{self, EndpointRole, EnrollmentRequest},
    service,
};
use crate::{
    account,
    crypto::{EncodedPublicKey, KeyPair, PublicKey},
    database, endpoint,
    sync::SharedMap,
    token::{self, VerifiedToken},
    Account, Database, Endpoint, Role, State, Token,
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
            role: EndpointRole::from(role) as i32,
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
    #[tracing::instrument(
        name = "send_enrollment", 
        skip_all,
        fields(
            public_key = %target.public_key,
            url = %target.host_address,
            role = %target.role,
            email = target.admin_email,
        )
    )]
    pub async fn send(target: Target, ourself: Issuer) -> Result<Self, Error> {
        let endpoint = endpoint::Id::generate();
        let account = account::Id::generate();

        debug!(%endpoint, %account, "Generated endpoint & account IDs for enrollment request");

        let account_token = create_account_token(endpoint, account, target.role, &ourself)?;

        let enrollment = Enrollment {
            endpoint,
            account,
            target,
            token: account_token.clone(),
        };

        let mut client = service::Client::connect(enrollment.target.host_address.clone()).await?;

        let resp = client
            .enroll(EnrollmentRequest {
                issuer: Some(ourself.into()),
                account_token: account_token.encoded,
                role: EndpointRole::from(enrollment.target.role) as i32,
            })
            .await;

        match resp {
            Ok(_) => {
                info!(
                    %endpoint,
                    %account,
                    public_key = %enrollment.target.public_key,
                    url = %enrollment.target.host_address,
                    role = %enrollment.target.role,
                    email = enrollment.target.admin_email,
                    "Enrollment request sent"
                );

                Ok(enrollment)
            }
            Err(error) => Err(Error::Grpc(error)),
        }
    }

    /// Accept the enrollment from the receiving side
    #[tracing::instrument(
        name = "accept_enrollment",
        skip_all,
        fields(
            endpoint = %self.endpoint,
            account = %self.account,
            public_key = %self.target.public_key,
            url = %self.target.host_address,
            role = %self.target.role,
            email = self.target.admin_email,
        )
    )]
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

        info!(username, "Created a new service account");

        let endpoint_id = self.endpoint;
        let kind = match self.target.role {
            Role::Builder => endpoint::Kind::Builder(endpoint::builder::Extension {
                admin_email: self.target.admin_email.clone(),
                admin_name: self.target.admin_name.clone(),
                description: self.target.description.clone(),
                work_status: endpoint::builder::WorkStatus::Idle,
            }),
            Role::RepositoryManager => endpoint::Kind::RepositoryManager,
            Role::Hub => endpoint::Kind::Hub,
        };

        let mut endpoint = Endpoint {
            id: endpoint_id,
            host_address: self.target.host_address.clone(),
            status: endpoint::Status::AwaitingAcceptance,
            error: None,
            account: account_id,
            kind,
        };
        endpoint.save(db).await.map_err(Error::CreateEndpoint)?;

        endpoint::Tokens {
            account_token: Some(self.token.encoded.clone()),
            api_token: None,
        }
        .save(db, endpoint.id)
        .await
        .map_err(Error::SetEndpointAccountToken)?;

        info!("Created a new endpoint for the service account");

        let account_token =
            create_account_token(endpoint_id, account_id, self.target.role, &ourself)?;

        account::Token::set(
            db,
            account_id,
            &account_token.encoded,
            account_token.expires(),
        )
        .await
        .map_err(Error::SetAccountToken)?;

        info!(
            expiration = %account_token.expires(),
            "Account token created",
        );

        let mut client =
            service::connect_with_auth(self.target.host_address, self.token.encoded).await?;

        let resp = client
            .accept(EnrollmentRequest {
                issuer: Some(ourself.into()),
                account_token: account_token.encoded,
                role: EndpointRole::from(self.target.role) as i32,
            })
            .await;

        match resp {
            Ok(_) => {
                endpoint.status = endpoint::Status::Operational;
                endpoint
                    .save(db)
                    .await
                    .map_err(Error::UpdateEndpointStatus)?;

                info!("Accepted endpoint now operational");

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
        let mut client =
            service::connect_with_auth(self.target.host_address, self.token.encoded).await?;

        client.decline(()).await?;

        Ok(())
    }

    /// Mark the enrollment as accepted from the sending side
    #[tracing::instrument(
        name = "accepted_enrollment",
        skip_all,
        fields(
            endpoint = %self.endpoint,
            account = %self.account,
            public_key = %self.target.public_key,
            url = %self.target.host_address,
            role = %self.target.role,
            email = self.target.admin_email,
        )
    )]
    pub async fn accepted(
        &self,
        db: &Database,
        public_key: &PublicKey,
        account_token: String,
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
            account_token: Some(account_token),
            api_token: None,
        }
        .save(db, endpoint)
        .await
        .map_err(Error::SetEndpointAccountToken)?;

        info!("Created a new endpoint for the service account");

        account::Token::set(db, self.account, &self.token.encoded, self.token.expires())
            .await
            .map_err(Error::SetAccountToken)?;

        info!(
            expiration = %self.token.expires(),
            "Account token saved",
        );

        info!("Accepted endpoint now operational");

        Ok(())
    }
}

#[tracing::instrument(skip_all)]
pub async fn send_initial_enrollment(
    target: Target,
    ourself: Issuer,
    state: State,
) -> Result<(), Error> {
    // If we're paired & operational, we don't need to resend
    for endpoint in Endpoint::list(&state.db)
        .await
        .map_err(Error::ListEndpoints)?
    {
        let account = Account::get(&state.db, endpoint.account)
            .await
            .map_err(Error::ReadAccount)?;

        if matches!(endpoint.status, endpoint::Status::Operational)
            && endpoint.host_address == target.host_address
            && account.public_key == target.public_key.encode()
        {
            debug!(
                url = %endpoint.host_address,
                public_key = %account.public_key,
                "Configured endpoint already operational"
            );
            return Ok(());
        }
    }

    let enrollment = Enrollment::send(target, ourself).await?;

    state
        .pending_enrollment
        .insert(enrollment.endpoint, enrollment)
        .await;

    Ok(())
}

fn create_account_token(
    endpoint: endpoint::Id,
    account: account::Id,
    role: Role,
    ourself: &Issuer,
) -> Result<VerifiedToken, Error> {
    let purpose = token::Purpose::Account;
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
    let account_token = token.sign(&ourself.key_pair)?;

    Ok(VerifiedToken {
        encoded: account_token,
        decoded: token,
    })
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("grpc request failed")]
    Grpc(#[from] tonic::Status),
    #[error("read account")]
    ReadAccount(#[source] account::Error),
    #[error("create service account")]
    CreateServiceAccount(#[source] account::Error),
    #[error("list endpoints")]
    ListEndpoints(#[source] database::Error),
    #[error("create endpoint")]
    CreateEndpoint(#[source] database::Error),
    #[error("set endpoint account token")]
    SetEndpointAccountToken(#[source] database::Error),
    #[error("set account token")]
    SetAccountToken(#[source] account::Error),
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
