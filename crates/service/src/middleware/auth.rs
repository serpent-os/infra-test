use bitflags::bitflags;
use log::warn;
use tonic::{body::BoxBody, transport::Body};

use crate::{
    account,
    crypto::PublicKey,
    token::{self, Validation, VerifiedToken},
    Token,
};

pub fn auth<T>(req: &tonic::Request<T>, validation_flags: Flags) -> Result<(), tonic::Status> {
    let flag_names = |flags: Flags| flags.iter_names().map(|(name, _)| name).collect::<Vec<_>>();

    let request_flags = req.extensions().get::<Flags>().copied().unwrap_or_default();

    let validation_names = flag_names(validation_flags);
    let token_names = flag_names(request_flags);

    // If token flags wholy contains all validation flags,
    // then user is properly authenticated
    if request_flags.contains(validation_flags) {
        Ok(())
    } else {
        warn!("authentication failed, expected {validation_names:?} got {token_names:?}");
        Err(tonic::Status::unauthenticated("authentication failed"))
    }
}

/// Middleware to extract auth token and decorate request with [`Flags`],
/// allowing downstream handlers to assess permissions via [`auth`] function.
///
/// If an auth token is on the request and verified using [`Verification`],
/// [`VerifiedToken`] will be added as an extension.
#[derive(Debug, Clone)]
pub struct Auth {
    pub pub_key: PublicKey,
    pub validation: Validation,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Flags : u16 {
        const NO_AUTH = 0;
        const BEARER_TOKEN = 1 << 0;
        const ACCESS_TOKEN = 1 << 1;
        const SERVICE_ACCOUNT = 1 << 2;
        const BOT_ACCOUNT = 1 << 3;
        const USER_ACCOUNT = 1 << 4;
        const ADMIN_ACCOUNT = 1 << 5;
        const EXPIRED = 1 << 6;
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
                token::Purpose::Authorization => flags |= Flags::BEARER_TOKEN,
                token::Purpose::Authentication => flags |= Flags::ACCESS_TOKEN,
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
            warn!("Invalid authorization token: {error}");
            None
        }
    }
}
