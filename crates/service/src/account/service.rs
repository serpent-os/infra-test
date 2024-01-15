use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use futures::{
    stream::{self, BoxStream},
    Stream, StreamExt,
};
use http::Uri;
use log::debug;
use rand::Rng;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport;

use crate::{
    account,
    crypto::{self, EncodedPublicKey, EncodedSignature, KeyPair, PublicKey},
    middleware::log::log_handler,
    token, Account, Database, Role, Token,
};

use super::proto::{
    self, account_service_server::AccountService, login_request, login_response, Credentials,
    LoginRequest, LoginResponse,
};

pub type Server = proto::account_service_server::AccountServiceServer<Service>;
pub type Client<T> = proto::account_service_client::AccountServiceClient<T>;

pub struct Service {
    pub db: Database,
    pub key_pair: KeyPair,
    pub role: Role,
}

impl Service {
    pub fn login(
        &self,
        request: tonic::Request<tonic::Streaming<LoginRequest>>,
    ) -> impl Stream<Item = Result<LoginResponse, Error>> + 'static {
        #[allow(clippy::large_enum_variant)]
        enum State {
            Idle {
                db: Database,
                key_pair: KeyPair,
                role: Role,
            },
            ChallengeSent {
                key_pair: KeyPair,
                role: Role,
                account: Account,
                public_key: PublicKey,
                challenge: String,
            },
            Finished,
        }

        let state = State::Idle {
            db: self.db.clone(),
            key_pair: self.key_pair.clone(),
            role: self.role,
        };

        stream::try_unfold(
            (request.into_inner(), state),
            |(mut incoming, state)| async move {
                let Some(request) = incoming.next().await else {
                    return Ok(None);
                };

                let body = request
                    .map_err(Error::Request)?
                    .body
                    .ok_or(Error::MalformedRequest)?;

                match (state, body) {
                    (
                        State::Idle { db, key_pair, role },
                        login_request::Body::Credentials(Credentials {
                            username,
                            public_key,
                        }),
                    ) => {
                        let public_key = EncodedPublicKey::decode(&public_key)
                            .map_err(Error::MalformedPublicKey)?;
                        let encoded_public_key = public_key.encode();

                        let account =
                            Account::lookup_with_credentials(&db, &username, &encoded_public_key)
                                .await
                                .map_err(|error| {
                                    Error::AccountLookup(
                                        username.clone(),
                                        encoded_public_key.clone(),
                                        error,
                                    )
                                })?;

                        let mut rand = rand::thread_rng();
                        let mut challenge = String::default();
                        base64::prelude::BASE64_STANDARD_NO_PAD
                            .encode_string(rand.gen::<[u8; 16]>(), &mut challenge);

                        debug!("Created login challenge for user {username}, public_key {encoded_public_key}: {challenge}");

                        Ok(Some((
                            LoginResponse {
                                body: Some(login_response::Body::Challenge(challenge.clone())),
                            },
                            (
                                incoming,
                                State::ChallengeSent {
                                    key_pair,
                                    role,
                                    account,
                                    public_key,
                                    challenge,
                                },
                            ),
                        )))
                    }
                    (
                        State::ChallengeSent {
                            key_pair,
                            role,
                            account,
                            public_key,
                            challenge,
                        },
                        login_request::Body::Signature(signature),
                    ) => {
                        let signature = EncodedSignature::decode(&signature)
                            .map_err(Error::MalformedSignature)?;

                        public_key
                            .verify(challenge.as_bytes(), &signature)
                            .map_err(Error::InvalidSignature)?;

                        let purpose = token::Purpose::Authentication;
                        let now = Utc::now();
                        let expires_on = now + purpose.duration();

                        let token = Token::new(token::Payload {
                            // TODO: How should we set these?
                            aud: account.id.to_string(),
                            exp: expires_on.timestamp(),
                            iat: now.timestamp(),
                            iss: role.service_name().to_string(),
                            sub: account.id.to_string(),
                            purpose,
                            account_id: account.id,
                            account_type: account.kind,
                        })
                        .sign(&key_pair)
                        .map_err(Error::SignToken)?;

                        debug!("Login successful for {}, issued token {token}", account.id);

                        Ok(Some((
                            LoginResponse {
                                body: Some(login_response::Body::Token(token)),
                            },
                            (incoming, State::Finished),
                        )))
                    }
                    _ => Err(Error::MalformedRequest),
                }
            },
        )
    }
}

#[tonic::async_trait]
impl AccountService for Service {
    type LoginStream = BoxStream<'static, Result<LoginResponse, tonic::Status>>;

    async fn login(
        &self,
        request: tonic::Request<tonic::Streaming<LoginRequest>>,
    ) -> Result<tonic::Response<Self::LoginStream>, tonic::Status> {
        Ok(tonic::Response::new(
            self.login(request)
                .map(|result| log_handler(result).map(tonic::Response::into_inner))
                .boxed(),
        ))
    }
}

pub async fn login(uri: Uri, username: String, key_pair: KeyPair) -> Result<String, ClientError> {
    let mut client = Client::connect(uri).await?;

    let (request_tx, request_rx) = mpsc::channel(1);
    let (challenge_tx, mut challenge_rx) = mpsc::channel::<String>(1);

    let request = ReceiverStream::new(request_rx);

    tokio::spawn(async move {
        let _ = request_tx
            .send(LoginRequest {
                body: Some(login_request::Body::Credentials(Credentials {
                    username,
                    public_key: key_pair.public_key().encode().to_string(),
                })),
            })
            .await;

        let Some(challenge) = challenge_rx.recv().await else {
            return;
        };

        let signature = base64::prelude::BASE64_STANDARD_NO_PAD
            .encode(key_pair.sign(challenge.as_bytes()).to_bytes());

        let _ = request_tx
            .send(LoginRequest {
                body: Some(login_request::Body::Signature(signature)),
            })
            .await;
    });

    let mut resp = client.login(request).await?.into_inner();

    let Some(login_response::Body::Challenge(challenge)) =
        resp.next().await.ok_or(ClientError::StreamClosed)??.body
    else {
        return Err(ClientError::MalformedRequest);
    };

    let _ = challenge_tx.send(challenge).await;

    let Some(login_response::Body::Token(token)) =
        resp.next().await.ok_or(ClientError::StreamClosed)??.body
    else {
        return Err(ClientError::MalformedRequest);
    };

    Ok(token)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Malformed request")]
    MalformedRequest,
    #[error("malformed public key")]
    MalformedPublicKey(#[source] crypto::Error),
    #[error("malformed signature")]
    MalformedSignature(#[source] crypto::Error),
    #[error("signature verification")]
    InvalidSignature(#[source] crypto::Error),
    #[error("account lookup for username {0}, public_key {1}")]
    AccountLookup(String, EncodedPublicKey, #[source] account::Error),
    #[error("sign token")]
    SignToken(#[source] token::Error),
    #[error(transparent)]
    Request(#[from] tonic::Status),
}

impl From<Error> for tonic::Status {
    fn from(error: Error) -> Self {
        let mut status = match &error {
            Error::MalformedRequest => tonic::Status::internal(""),
            Error::SignToken(_) => tonic::Status::internal(""),
            Error::MalformedPublicKey(_) => tonic::Status::invalid_argument("malformed public key"),
            Error::MalformedSignature(_) => tonic::Status::invalid_argument("malformed signature"),
            Error::InvalidSignature(_) => tonic::Status::unauthenticated(""),
            Error::AccountLookup(..) => tonic::Status::unauthenticated(""),
            Error::Request(status) => status.clone(),
        };
        status.set_source(Arc::new(error));
        status
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Malformed request")]
    MalformedRequest,
    #[error("Stream closed")]
    StreamClosed,
    #[error("transport")]
    Transport(#[from] transport::Error),
    #[error(transparent)]
    Request(#[from] tonic::Status),
}
