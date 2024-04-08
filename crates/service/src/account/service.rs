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
    middleware::log_handler,
    token, Account, Database, Role, Token,
};

pub use super::proto::TokenResponse;
use super::proto::{
    self, account_service_server::AccountService, authenticate_request, authenticate_response,
    AuthenticateRequest, AuthenticateResponse, Credentials, RefreshTokenRequest,
};

pub type Server = proto::account_service_server::AccountServiceServer<Service>;
pub type Client<T> = proto::account_service_client::AccountServiceClient<T>;

pub struct Service {
    pub db: Database,
    pub key_pair: KeyPair,
    pub role: Role,
}

impl Service {
    pub fn authenticate(
        &self,
        request: tonic::Request<tonic::Streaming<AuthenticateRequest>>,
    ) -> impl Stream<Item = Result<AuthenticateResponse, Error>> + 'static {
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
                        authenticate_request::Body::Credentials(Credentials {
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

                        debug!("Created authenticate challenge for user {username}, public_key {encoded_public_key}: {challenge}");

                        Ok(Some((
                            AuthenticateResponse {
                                body: Some(authenticate_response::Body::Challenge(
                                    challenge.clone(),
                                )),
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
                        authenticate_request::Body::Signature(signature),
                    ) => {
                        let signature = EncodedSignature::decode(&signature)
                            .map_err(Error::MalformedSignature)?;

                        public_key
                            .verify(challenge.as_bytes(), &signature)
                            .map_err(Error::InvalidSignature)?;

                        let account_token =
                            create_token(&key_pair, &account, role, token::Purpose::Account)?;
                        let api_token =
                            create_token(&key_pair, &account, role, token::Purpose::Api)?;

                        debug!(
                            "Authenticate successful for {}, issued account_token {account_token}, api_token: {api_token}",
                            account.id
                        );

                        Ok(Some((
                            AuthenticateResponse {
                                body: Some(authenticate_response::Body::Tokens(TokenResponse {
                                    account_token,
                                    api_token,
                                })),
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
    type AuthenticateStream = BoxStream<'static, Result<AuthenticateResponse, tonic::Status>>;

    async fn authenticate(
        &self,
        request: tonic::Request<tonic::Streaming<AuthenticateRequest>>,
    ) -> Result<tonic::Response<Self::AuthenticateStream>, tonic::Status> {
        Ok(tonic::Response::new(
            self.authenticate(request)
                .map(|result| log_handler(result).map(tonic::Response::into_inner))
                .boxed(),
        ))
    }

    async fn refresh_token(
        &self,
        _request: tonic::Request<RefreshTokenRequest>,
    ) -> Result<tonic::Response<TokenResponse>, tonic::Status> {
        todo!();
    }
}

pub async fn authenticate(
    uri: Uri,
    username: String,
    key_pair: KeyPair,
) -> Result<TokenResponse, ClientError> {
    let mut client = Client::connect(uri).await?;

    let (request_tx, request_rx) = mpsc::channel(1);
    let (challenge_tx, mut challenge_rx) = mpsc::channel::<String>(1);

    let request = ReceiverStream::new(request_rx);

    tokio::spawn(async move {
        let _ = request_tx
            .send(AuthenticateRequest {
                body: Some(authenticate_request::Body::Credentials(Credentials {
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
            .send(AuthenticateRequest {
                body: Some(authenticate_request::Body::Signature(signature)),
            })
            .await;
    });

    let mut resp = client.authenticate(request).await?.into_inner();

    let Some(authenticate_response::Body::Challenge(challenge)) =
        resp.next().await.ok_or(ClientError::StreamClosed)??.body
    else {
        return Err(ClientError::MalformedRequest);
    };

    let _ = challenge_tx.send(challenge).await;

    let Some(authenticate_response::Body::Tokens(tokens)) =
        resp.next().await.ok_or(ClientError::StreamClosed)??.body
    else {
        return Err(ClientError::MalformedRequest);
    };

    Ok(tokens)
}

fn create_token(
    key_pair: &KeyPair,
    account: &Account,
    role: Role,
    purpose: token::Purpose,
) -> Result<String, Error> {
    let now = Utc::now();
    let expires_on = now + purpose.duration();

    Token::new(token::Payload {
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
    .sign(key_pair)
    .map_err(Error::SignToken)
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
