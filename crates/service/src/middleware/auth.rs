//! Parse the authorization token from incoming requests, validate it and provide
//! the verified token & flags as extensions to downstream middleware / handlers

use bitflags::bitflags;
use tonic::{body::BoxBody, transport::Body};
use tracing::{debug, warn};

use crate::{
    account,
    crypto::PublicKey,
    token::{self, Validation, VerifiedToken},
    Token,
};

/// Validate the provided `request` against `validation_flags` and return an
/// error if the request doesn't pass the required validation.
pub fn auth<T>(request: &tonic::Request<T>, validation_flags: Flags) -> Result<(), tonic::Status> {
    let request_flags = request
        .extensions()
        .get::<Flags>()
        .copied()
        .unwrap_or_default();

    let validation_names = flag_names(validation_flags);
    let token_names = flag_names(request_flags);

    // If token flags wholy contains all validation flags,
    // then user is properly authorized
    if request_flags.contains(validation_flags) {
        Ok(())
    } else if request_flags == Flags::NO_AUTH {
        warn!(expected = ?validation_names, received = ?token_names, "unauthenticated");
        Err(tonic::Status::unauthenticated("unauthenticated"))
    } else {
        warn!(expected = ?validation_names, received = ?token_names, "permission denied");
        Err(tonic::Status::permission_denied("permission denied"))
    }
}

/// Middleware to extract auth token and decorate request with [`Flags`],
/// allowing downstream handlers to assess permissions via [`auth`] function.
///
/// If an auth token is on the request and verified using [`Validation`],
/// [`VerifiedToken`] will be added as an extension.
#[derive(Debug, Clone)]
pub struct Auth {
    /// Public key used to verify the [`Token`] signature
    pub pub_key: PublicKey,
    /// Validation rules used when calling [`Token::verify`]
    pub validation: Validation,
}

bitflags! {
    /// Authorization flags that describe the account making the request
    #[derive(Debug, Clone, Copy,PartialEq, Eq, Default)]
    pub struct Flags : u16 {
        /// Missing or invalid token
        const NO_AUTH = 0;
        /// Account token purpose
        const ACCOUNT_TOKEN = 1 << 0;
        /// API token purpose
        const API_TOKEN = 1 << 1;
        /// Service account type
        const SERVICE_ACCOUNT = 1 << 2;
        /// Bot account type
        const BOT_ACCOUNT = 1 << 3;
        /// User account type
        const USER_ACCOUNT = 1 << 4;
        /// Admin account type
        const ADMIN_ACCOUNT = 1 << 5;
        /// Token is expired
        const EXPIRED = 1 << 6;
        /// Token is not expired
        const NOT_EXPIRED = 1 << 7;
    }
}

impl<S> tower::Layer<S> for Auth {
    type Service = Service<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Service {
            inner,
            pub_key: self.pub_key,
            validation: self.validation.clone(),
        }
    }
}

/// Tower service of the [`Auth`] layer
#[derive(Debug, Clone)]
pub struct Service<S> {
    inner: S,
    pub_key: PublicKey,
    validation: Validation,
}

impl<S> tower::Service<http::Request<Body>> for Service<S>
where
    S: tower::Service<http::Request<Body>, Response = http::Response<BoxBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        tower::Service::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, mut req: http::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let token_maybe = extract_token(&req, &self.pub_key, &self.validation);

        let mut flags = Flags::default();

        if let Some(token) = token_maybe {
            req.extensions_mut().insert(token.clone());

            match token.decoded.payload.purpose {
                token::Purpose::Account => flags |= Flags::ACCOUNT_TOKEN,
                token::Purpose::Api => flags |= Flags::API_TOKEN,
            }

            match token.decoded.payload.account_type {
                account::Kind::Admin => flags |= Flags::ADMIN_ACCOUNT,
                account::Kind::Standard => flags |= Flags::USER_ACCOUNT,
                account::Kind::Bot => flags |= Flags::BOT_ACCOUNT,
                account::Kind::Service => flags |= Flags::SERVICE_ACCOUNT,
            }

            if token.decoded.is_expired() {
                flags |= Flags::EXPIRED
            } else {
                flags |= Flags::NOT_EXPIRED
            }

            let token_flags = flag_names(flags);
            let token_purpose = Some(token.decoded.payload.purpose.to_string());
            let account = Some(token.decoded.payload.account_id.to_string());
            let account_type = Some(token.decoded.payload.account_type.to_string());

            debug!(
                ?token_flags,
                token_purpose, account, account_type, "Auth parsed"
            );
        }

        req.extensions_mut().insert(flags);

        inner.call(req)
    }
}

fn extract_token(
    req: &http::Request<Body>,
    pub_key: &PublicKey,
    validation: &Validation,
) -> Option<VerifiedToken> {
    let header = req.headers().get("authorization")?;
    let token_str = header.to_str().ok()?.strip_prefix("Bearer ")?;

    match Token::verify(token_str, pub_key, validation) {
        Ok(token) => Some(token),
        Err(error) => {
            warn!(%error, "Invalid authorization token");
            None
        }
    }
}

fn flag_names(flags: Flags) -> Vec<String> {
    flags
        .iter_names()
        .map(|(name, _)| name.to_string())
        .collect()
}
