//! Make requests to service APIs
use std::{any, convert::Infallible, sync::LazyLock, time::Duration};

use http::Uri;
use service_core::auth;
use thiserror::Error;
use tracing::{error, info};

use crate::{
    account, api,
    crypto::{self, PublicKey},
    database, endpoint,
    token::{self, VerifiedToken},
    Account, Database, Endpoint, Token,
};

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::ClientBuilder::new()
        .referer(false)
        // TODO: What should this be?
        .user_agent(concat!("serpentos-infra-client", "/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("build reqwest client")
});

const TOKEN_VALIDITY: Duration = Duration::from_secs(15 * 60);

/// A service client
#[derive(Clone)]
pub struct Client<A = NoAuth> {
    host_address: Uri,
    auth_storage: A,
}

impl Client {
    /// Create a client for the provided address
    pub fn new(host_address: Uri) -> Self {
        Self {
            host_address,
            auth_storage: NoAuth,
        }
    }
}

impl<A> Client<A>
where
    A: AuthStorage,
    A::Error: std::error::Error,
{
    /// Use custom [`AuthStorage`] with this client
    pub fn with_auth<T>(self, storage: T) -> Client<T> {
        Client {
            auth_storage: storage,
            host_address: self.host_address,
        }
    }

    /// Use [`TokensAuth`] with this client
    pub fn with_tokens(self, tokens: Tokens) -> Client<TokensAuth> {
        Client {
            auth_storage: TokensAuth(tokens),
            host_address: self.host_address,
        }
    }

    /// Use [`EndpointAuth`] with this client
    pub fn with_endpoint_auth(self, endpoint: endpoint::Id, db: Database) -> Client<EndpointAuth> {
        Client {
            auth_storage: EndpointAuth { endpoint, db },
            host_address: self.host_address,
        }
    }

    /// Send a request to an [`api::Operation`]
    #[tracing::instrument(
        skip_all,
        fields(
            url = %self.host_address,
            path = O::PATH,
        )
    )]
    pub async fn send<O>(&self, body: &O::RequestBody) -> Result<O::ResponseBody, Error<A::Error>>
    where
        O: api::Operation + 'static,
    {
        let mut token = None;

        // Does request we need auth?
        if O::AUTH.intersects(auth::Flags::ACCESS_TOKEN | auth::Flags::BEARER_TOKEN) {
            let mut tokens = self.auth_storage.tokens().await.map_err(Error::AuthStorage)?;

            // If storage supports persisting refresh tokens, ensure they're refreshed
            if A::REFRESH_ENABLED {
                let bearer_token = tokens.bearer_token.clone().ok_or(Error::MissingBearerToken)?;

                if bearer_token.decoded.is_expired_in(TOKEN_VALIDITY) {
                    tokens = self
                        .refresh_token(token::Purpose::Authorization, &bearer_token.encoded)
                        .await?;
                }
                if tokens.access_token.is_none()
                    || tokens
                        .access_token
                        .as_ref()
                        .is_some_and(|token| token.decoded.is_expired_in(TOKEN_VALIDITY))
                {
                    tokens = self
                        .refresh_token(token::Purpose::Authentication, &bearer_token.encoded)
                        .await?;
                }
            }

            // Select proper token for the request
            token = Some(if O::AUTH.contains(auth::Flags::BEARER_TOKEN) {
                tokens
                    .bearer_token
                    .as_ref()
                    .ok_or(Error::MissingBearerToken)?
                    .encoded
                    .clone()
            } else {
                tokens
                    .access_token
                    .as_ref()
                    .ok_or(Error::MissingAccessToken)?
                    .encoded
                    .clone()
            });
        }

        Ok(self.raw_send::<O>(body, token.as_deref()).await?)
    }

    async fn raw_send<O>(&self, body: &O::RequestBody, token: Option<&str>) -> Result<O::ResponseBody, reqwest::Error>
    where
        O: api::Operation + 'static,
    {
        let mut request = CLIENT.request(
            O::METHOD,
            format!("{}api/{}/{}", self.host_address, O::VERSION, O::PATH),
        );

        if let Some(token) = token {
            request = request.bearer_auth(token);
        }

        // Send () as empty body
        if any::TypeId::of::<O::RequestBody>() == any::TypeId::of::<()>() {
            request = request.body(reqwest::Body::default());
        } else {
            request = request.json(body);
        }

        let resp = CLIENT.execute(request.build()?).await?.error_for_status()?;

        // Support empty body into ()
        if any::TypeId::of::<O::ResponseBody>() == any::TypeId::of::<()>() {
            Ok(serde_json::from_slice(b"null").expect("null is ()"))
        } else {
            resp.json::<O::ResponseBody>().await
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            url = %self.host_address,
            purpose = %purpose,
        )
    )]
    async fn refresh_token(&self, purpose: token::Purpose, bearer: &str) -> Result<Tokens, Error<A::Error>> {
        let resp = match purpose {
            token::Purpose::Authorization => {
                self.raw_send::<api::v1::services::RefreshIssueToken>(&(), Some(bearer))
                    .await
            }
            token::Purpose::Authentication => {
                self.raw_send::<api::v1::services::RefreshToken>(&(), Some(bearer))
                    .await
            }
        };

        match resp {
            Ok(token) => self
                .auth_storage
                .token_refreshed(purpose, &token)
                .await
                .map_err(Error::AuthStorage),
            Err(e) => {
                self.auth_storage
                    .token_refresh_failed(purpose, &e)
                    .await
                    .map_err(Error::AuthStorage)?;

                Err(Error::Reqwest(e))
            }
        }
    }
}

/// A client error
#[derive(Debug, Error)]
pub enum Error<E = Infallible>
where
    E: std::error::Error,
{
    /// Missing bearer token
    #[error("Missing bearer token")]
    MissingBearerToken,
    /// Missing access token
    #[error("Missing access token")]
    MissingAccessToken,
    /// Failed to refresh bearer token
    #[error("Failed to refresh bearer token")]
    RefreshBearerTokenFailed,
    /// Failed to refresh access token
    #[error("Failed to refresh access token")]
    RefreshAccessTokenFailed,
    /// Auth storage error
    #[error("auth storage")]
    AuthStorage(#[source] E),
    /// Reqwest error
    #[error("reqwest")]
    Reqwest(#[from] reqwest::Error),
}

/// Tokens needed to make authenticated requests
#[derive(Debug, Clone, Default)]
pub struct Tokens {
    /// A bearer token
    pub bearer_token: Option<VerifiedToken>,
    /// An access token
    pub access_token: Option<VerifiedToken>,
}

/// A provider of token storage and possibly persistence
/// to enable automatic token refreshing
#[allow(async_fn_in_trait)]
pub trait AuthStorage {
    /// An auth storage error
    type Error;

    /// Can this storage persist refreshed tokens?
    ///
    /// Must be set true for [`Client`] to call [`AuthStorage::token_refreshed`]
    /// after an expired token is refreshed.
    const REFRESH_ENABLED: bool = false;

    /// Returns current tokens from this storage
    async fn tokens(&self) -> Result<Tokens, Self::Error>;
    /// Called when [`Client`] fetches a refresh token, allowing storage to persist
    /// the new token.
    ///
    /// Returns the current tokens.
    async fn token_refreshed(&self, _purpose: token::Purpose, _token: &str) -> Result<Tokens, Self::Error> {
        Ok(Tokens::default())
    }
    /// Called when [`Client`] fails to refresh a token
    async fn token_refresh_failed(&self, _purpose: token::Purpose, _error: &reqwest::Error) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// No auth storage is provided
///
/// Will only work with unauthenticated operations
pub struct NoAuth;

impl AuthStorage for NoAuth {
    type Error = Infallible;

    async fn tokens(&self) -> Result<Tokens, Self::Error> {
        Ok(Tokens::default())
    }
}

/// Auth with static tokens and no refresh persistence
pub struct TokensAuth(Tokens);

impl AuthStorage for TokensAuth {
    type Error = Infallible;

    async fn tokens(&self) -> Result<Tokens, Self::Error> {
        Ok(self.0.clone())
    }
}

/// Auth credentials are stored in [`Database`] for a configured endpoint
/// and updated when refresh tokens are fetched
pub struct EndpointAuth {
    endpoint: endpoint::Id,
    db: Database,
}

impl EndpointAuth {
    async fn verified_tokens(&self, public_key: &PublicKey) -> Result<Tokens, EndpointAuthError> {
        let tokens = endpoint::Tokens::get(&self.db, self.endpoint).await?;

        Ok(Tokens {
            bearer_token: tokens
                .bearer_token
                .as_deref()
                .map(|token| Token::verify(token, public_key, &token::Validation::new()))
                .transpose()?,
            access_token: tokens
                .access_token
                .as_deref()
                .map(|token| Token::verify(token, public_key, &token::Validation::new()))
                .transpose()?,
        })
    }
}

impl AuthStorage for EndpointAuth {
    type Error = EndpointAuthError;

    const REFRESH_ENABLED: bool = true;

    async fn tokens(&self) -> Result<Tokens, EndpointAuthError> {
        let endpoint = Endpoint::get(&self.db, self.endpoint).await?;
        let account = Account::get(&self.db, endpoint.account).await?;

        let public_key = account.public_key.decoded()?;

        self.verified_tokens(&public_key).await
    }

    #[tracing::instrument(
        skip_all,
        fields(
            endpoint = %self.endpoint,
            purpose = %purpose,
        )
    )]
    async fn token_refreshed(&self, purpose: token::Purpose, token: &str) -> Result<Tokens, Self::Error> {
        let mut endpoint = Endpoint::get(&self.db, self.endpoint).await?;
        let account = Account::get(&self.db, endpoint.account).await?;

        let public_key = account.public_key.decoded()?;

        match Token::verify(token, &public_key, &token::Validation::new()) {
            Ok(token) => {
                let mut tokens = self.verified_tokens(&public_key).await?;

                endpoint.status = endpoint::Status::Operational;
                endpoint.error = None;

                match purpose {
                    token::Purpose::Authorization => tokens.bearer_token = Some(token),
                    token::Purpose::Authentication => tokens.access_token = Some(token),
                }

                endpoint::Tokens {
                    bearer_token: tokens.bearer_token.as_ref().map(|token| token.encoded.clone()),
                    access_token: tokens.access_token.as_ref().map(|token| token.encoded.clone()),
                }
                .save(&self.db, self.endpoint)
                .await?;
                endpoint.save(&self.db).await?;

                info!("Token refreshed, endpoint operational");

                Ok(tokens)
            }
            Err(token::Error::InvalidSignature) => {
                endpoint.status = endpoint::Status::Forbidden;
                endpoint.error = Some("Invalid signature".to_string());

                error!("Invalid signature");

                endpoint.save(&self.db).await?;

                Err(EndpointAuthError::InvalidRefreshToken)
            }
            Err(_) => {
                endpoint.status = endpoint::Status::Forbidden;
                endpoint.error = Some("Invalid token".to_string());

                error!("Invalid token");

                endpoint.save(&self.db).await?;

                Err(EndpointAuthError::InvalidRefreshToken)
            }
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            endpoint = %self.endpoint,
            purpose = %purpose,
        )
    )]
    async fn token_refresh_failed(&self, purpose: token::Purpose, error: &reqwest::Error) -> Result<(), Self::Error> {
        let mut endpoint = Endpoint::get(&self.db, self.endpoint).await?;

        endpoint.status = endpoint::Status::Unreachable;

        match purpose {
            token::Purpose::Authorization => {
                endpoint.error = Some("Failed to refresh bearer token".to_string());
                error!(%error, "Failed to refresh bearer token");
            }
            token::Purpose::Authentication => {
                endpoint.error = Some("Failed to refresh access token".to_string());
                error!(%error, "Failed to refresh access token");
            }
        }

        endpoint.save(&self.db).await?;

        Ok(())
    }
}

/// An endpoint auth storage error
#[derive(Debug, Error)]
pub enum EndpointAuthError {
    /// Invalid refresh token
    #[error("Invalid refresh token")]
    InvalidRefreshToken,
    /// Account error
    #[error("account")]
    Account(#[from] account::Error),
    /// Crypto error
    #[error("crypto")]
    Crypto(#[from] crypto::Error),
    /// Database error
    #[error("database")]
    Database(#[from] database::Error),
    /// Error decoding token
    #[error("decode token")]
    DecodeToken(#[from] token::Error),
}
