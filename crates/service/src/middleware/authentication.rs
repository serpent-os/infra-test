use std::convert::Infallible;

use bitflags::bitflags;
use futures::{future::BoxFuture, FutureExt};
use tonic::{body::BoxBody, transport::Body};

use crate::{account, token, Token};

/// Middleware to validate the request authenticates against
/// the provided [`Flags`].
///
/// This middleware requires [`ExtractToken`] is slotted before it.
pub struct Authentication {
    pub flags: Flags,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Flags : u16 {
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

impl<S> tower::Layer<S> for Authentication {
    type Service = Service<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Service {
            inner,
            flags: self.flags,
        }
    }
}

pub struct Service<S> {
    inner: S,
    flags: Flags,
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
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        tower::Service::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let flag_names =
            |flags: Flags| flags.iter_names().map(|(name, _)| name).collect::<Vec<_>>();

        let validation_flags = self.flags;
        let mut token_flags = Flags::default();

        if let Some(token) = req.extensions().get::<Option<Token>>().cloned().flatten() {
            match token.payload.purpose {
                token::Purpose::Authorize => token_flags |= Flags::BEARER_TOKEN,
                token::Purpose::Authenticate => token_flags |= Flags::ACCESS_TOKEN,
            }

            match token.payload.account_type {
                account::Kind::Admin => token_flags |= Flags::ADMIN_ACCOUNT,
                account::Kind::Standard => token_flags |= Flags::USER_ACCOUNT,
                account::Kind::Bot => token_flags |= Flags::BOT_ACCOUNT,
                account::Kind::Service => token_flags |= Flags::SERVICE_ACCOUNT,
            }

            if token.is_expired() {
                token_flags |= Flags::EXPIRED
            } else {
                token_flags |= Flags::NOT_EXPIRED
            }
        }

        let validation_names = flag_names(validation_flags);
        let token_names = flag_names(token_flags);

        async move {
            // If token flags wholy contains all validation flags,
            // then user is properly authenticated
            if token_flags.contains(validation_flags) {
                inner.call(req).await
            } else {
                eprintln!(
                    "[warn] authentication failed, expected {validation_names:?} got {token_names:?}",
                );
                Ok(tonic::Status::unauthenticated("authentication failed").to_http())
            }
        }
        .boxed()
    }
}
