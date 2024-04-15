//! A [`Service`] implementing the [`AccountService`] interface
use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Utc};
use futures::{
    stream::{self, BoxStream},
    Stream, StreamExt,
};
use http::Uri;
use rand::Rng;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport;
use tracing::{debug, info_span};
use tracing_futures::Instrument;

use crate::{
    account,
    crypto::{self, EncodedPublicKey, EncodedSignature, KeyPair, PublicKey},
    middleware::{auth, log_handler},
    token::{self, VerifiedToken},
    Account, Database, Role, Token,
};

pub use super::proto::account_service_server::AccountService;
pub use super::proto::TokenResponse;
use super::proto::{
    self, authenticate_request, authenticate_response, AuthenticateRequest, AuthenticateResponse,
    Credentials,
};

/// A gRPC server capable of routing [`AccountService`] requests to be handled by [`Service`]
pub type Server = proto::account_service_server::AccountServiceServer<Service>;
/// A client that can connect to and call the [`AccountService`] interface
pub type Client<T> = proto::account_service_client::AccountServiceClient<T>;

/// An implementation of the [`AccountService`] interface
pub struct Service {
    /// Shared database of this service
    pub db: Database,
    /// Key pair of this service
    pub key_pair: KeyPair,
    /// Role of this service
    pub role: Role,
}

impl Service {
    fn authenticate(
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
                db: Database,
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

                        debug!(account = %account.id, challenge, "Authenticate challenge created");

                        Ok(Some((
                            AuthenticateResponse {
                                body: Some(authenticate_response::Body::Challenge(
                                    challenge.clone(),
                                )),
                            },
                            (
                                incoming,
                                State::ChallengeSent {
                                    db,
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
                            db,
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

                        let (account_token, expires_on) =
                            create_token(&key_pair, &account, role, token::Purpose::Account)?;
                        let (api_token, _) =
                            create_token(&key_pair, &account, role, token::Purpose::Api)?;

                        account::Token::set(&db, account.id, &account_token, expires_on)
                            .await
                            .map_err(Error::SaveAccountToken)?;

                        debug!(
                            account = %account.id,
                            "Authenticate successful",
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
        .instrument(info_span!("authenticate"))
    }

    async fn refresh_token(&self, request: tonic::Request<()>) -> Result<TokenResponse, Error> {
        let request_token = request
            .extensions()
            .get::<VerifiedToken>()
            .cloned()
            .ok_or(Error::MissingRequestToken)?;

        let token::Payload { account_id, .. } = request_token.decoded.payload;

        let account = Account::get(&self.db, account_id)
            .await
            .map_err(Error::ReadAccount)?;

        // Confirm this is their current account token
        let current_token = account::Token::get(&self.db, account_id)
            .await
            .map_err(Error::ReadAccountToken)?;

        if current_token.encoded != request_token.encoded {
            return Err(Error::NotCurrentAccountToken);
        }

        // We've already validated it's not expired in auth middleware
        // Looks good! Let's issue a new pair

        let (account_token, expires_on) =
            create_token(&self.key_pair, &account, self.role, token::Purpose::Account)?;
        let (api_token, _) =
            create_token(&self.key_pair, &account, self.role, token::Purpose::Api)?;

        // Update their account token to the newly issued one
        account::Token::set(&self.db, account_id, &account_token, expires_on)
            .await
            .map_err(Error::SaveAccountToken)?;

        debug!(
            account = %account.id,
            "Refresh token successful",
        );

        Ok(TokenResponse {
            account_token,
            api_token,
        })
    }
}

#[tonic::async_trait]
impl AccountService for Service {
    type AuthenticateStream = BoxStream<'static, Result<AuthenticateResponse, tonic::Status>>;

    async fn authenticate(
        &self,
        request: tonic::Request<tonic::Streaming<AuthenticateRequest>>,
    ) -> Result<tonic::Response<Self::AuthenticateStream>, tonic::Status> {
        // We return a stream so we trace the stream separately since
        // it's returned from this function and we don't want it to be
        // spanned twice
        info_span!("authenticate").in_scope(|| {
            // Technically the same as ommitting this check
            auth(&request, auth::Flags::NO_AUTH)
        })?;

        Ok(tonic::Response::new(
            self.authenticate(request)
                .map(|result| log_handler(result).map(tonic::Response::into_inner))
                .instrument(info_span!("authenticate"))
                .boxed(),
        ))
    }

    #[tracing::instrument(skip_all)]
    async fn refresh_token(
        &self,
        request: tonic::Request<()>,
    ) -> Result<tonic::Response<TokenResponse>, tonic::Status> {
        // Must have a non-expired account token
        auth(
            &request,
            auth::Flags::ACCOUNT_TOKEN | auth::Flags::NOT_EXPIRED,
        )?;

        log_handler(self.refresh_token(request).await)
    }
}

/// Authenticate with the [`AccountService`] at [`Uri`] using the provided `username` and `KeyPair` credentials.
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
) -> Result<(String, DateTime<Utc>), Error> {
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
    .sign(key_pair)
    .map_err(Error::SignToken)?;

    Ok((token, expires_on))
}

/// An error when handling an [`AccountService`] request
#[derive(Debug, Error)]
pub enum Error {
    /// Account not issued an account token
    #[error("Account not issued an account token")]
    NoIssuedAccountToken,
    /// Request token doesn't match current account token
    #[error("Request token doesn't match current account token")]
    NotCurrentAccountToken,
    /// Token missing from request
    #[error("Token missing from request")]
    MissingRequestToken,
    /// Request is malformed
    #[error("Malformed request")]
    MalformedRequest,
    /// Public key cannot be decoded
    #[error("malformed public key")]
    MalformedPublicKey(#[source] crypto::Error),
    /// Challenge signature cannot be decoded
    #[error("malformed signature")]
    MalformedSignature(#[source] crypto::Error),
    /// Signature is not valid
    #[error("signature verification")]
    InvalidSignature(#[source] crypto::Error),
    /// Saving account token failed
    #[error("saving new account token")]
    SaveAccountToken(#[source] account::Error),
    /// Reading account token failed
    #[error("reading account token")]
    ReadAccountToken(#[source] account::Error),
    /// Reading [`Account`] failed
    #[error("reading account")]
    ReadAccount(#[source] account::Error),
    /// Account lookup by credentials failed
    #[error("account lookup for username {0}, public_key {1}")]
    AccountLookup(String, EncodedPublicKey, #[source] account::Error),
    /// Signing token failed
    #[error("sign token")]
    SignToken(#[source] token::Error),
    /// gRPC request failed
    #[error(transparent)]
    Request(#[from] tonic::Status),
}

impl From<Error> for tonic::Status {
    fn from(error: Error) -> Self {
        let mut status = match &error {
            Error::NoIssuedAccountToken => tonic::Status::unauthenticated(""),
            Error::NotCurrentAccountToken => tonic::Status::unauthenticated(""),
            Error::MissingRequestToken => tonic::Status::internal(""),
            Error::MalformedRequest => tonic::Status::internal(""),
            Error::SignToken(_) => tonic::Status::internal(""),
            Error::SaveAccountToken(_) => tonic::Status::internal(""),
            Error::ReadAccountToken(_) => tonic::Status::internal(""),
            Error::ReadAccount(_) => tonic::Status::internal(""),
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

/// A [`Client`] error
#[derive(Debug, Error)]
pub enum ClientError {
    /// Request is malformed
    #[error("Malformed request")]
    MalformedRequest,
    /// Stream closed
    #[error("Stream closed")]
    StreamClosed,
    /// gRPC transport error
    #[error("transport")]
    Transport(#[from] transport::Error),
    /// gRPC request failed
    #[error(transparent)]
    Request(#[from] tonic::Status),
}
