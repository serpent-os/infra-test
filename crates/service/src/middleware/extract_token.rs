use std::convert::Infallible;

use tonic::{body::BoxBody, transport::Body};

use crate::{crypto::PublicKey, token::Validation, Token};

/// Extract's an incoming authorization token and verifies
/// it using the provided public key and validation.
///
/// `Option<Token>` is made available as a request extension and
/// set if the token is valid.
pub struct ExtractToken {
    pub pub_key: PublicKey,
    pub validation: Validation,
}

impl<S> tower::Layer<S> for ExtractToken {
    type Service = Service<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Service {
            inner,
            pub_key: self.pub_key,
            validation: self.validation.clone(),
        }
    }
}

pub struct Service<S> {
    inner: S,
    pub_key: PublicKey,
    validation: Validation,
}

impl<S> tower::Service<http::Request<Body>> for Service<S>
where
    S: tower::Service<http::Request<Body>, Response = http::Response<BoxBody>, Error = Infallible>
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
        req.extensions_mut().insert(token_maybe);

        inner.call(req)
    }
}

fn extract_token(
    req: &http::Request<Body>,
    pub_key: &PublicKey,
    validation: &Validation,
) -> Option<Token> {
    let header = req.headers().get("authorization")?;
    let token_str = header.to_str().ok()?.strip_prefix("Bearer ")?;

    match Token::verify(token_str, pub_key, validation) {
        Ok(token) => Some(token),
        Err(error) => {
            // TODO: Log error
            eprintln!("[error] Invalid authorization token: {error}");
            None
        }
    }
}
