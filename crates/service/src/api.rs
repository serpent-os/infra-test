//! Register API routes
use std::{any, marker::PhantomData};

use axum::{
    extract::{FromRequest, FromRequestParts, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{MethodFilter, MethodRouter},
    Json, Router,
};
use futures_util::{future::BoxFuture, FutureExt};

use serde::Serialize;
use service_core::auth;
use tracing::warn;

use crate::{middleware, token::VerifiedToken};

pub use service_core::api::{
    operation::{self, Operation},
    Version,
};

pub use self::handler::Handler;

pub mod handler;
pub mod v1;

type RawRequest = axum::extract::Request;
type RawResponse = axum::response::Response;

/// Register API operations with handlers
pub struct Service<S = ()> {
    router: Router<S>,
}

impl<S> Default for Service<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self { router: Router::new() }
    }
}

impl<S> Service<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Return a new [`Service`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a [`Handler`] to an [`Operation`]
    pub fn register<O, E, H>(mut self, handler: H) -> Self
    where
        O: Operation + 'static,
        H: Handler<O, S> + Clone + Send + Sync + 'static,
        <H as Handler<O, S>>::Error: std::error::Error + Send + Sync + 'static,
        StatusCode: for<'a> From<&'a <H as Handler<O, S>>::Error>,
    {
        let filter = MethodFilter::try_from(O::METHOD).expect("unknown method");

        self.router = self.router.route(
            &format!("/api/{}/{}", O::VERSION, O::PATH),
            MethodRouter::new().on(filter, OperationHandler::new(handler)),
        );
        self
    }

    /// Make state available to all registered handlers
    pub fn with_state(self, state: S) -> Service<()> {
        Service {
            router: self.router.with_state(state),
        }
    }

    pub(crate) fn into_router(self) -> Router<S> {
        self.router
    }
}

/// A request passed to an [`Operation`]
pub struct Request<O>
where
    O: Operation,
{
    /// Request headers
    pub headers: HeaderMap,
    /// Request body
    pub body: O::RequestBody,
    /// Extracted token
    ///
    /// This will always be set if [`auth::Flags`]
    /// are set requiring a token, otherwise the
    /// request will be rejected before reaching
    /// it's [`Handler`]
    pub token: Option<VerifiedToken>,
}

#[derive(Debug)]
struct OperationHandler<O, H, S> {
    handler: H,
    _marker: PhantomData<fn() -> (O, S)>,
}

impl<O, H, S> OperationHandler<O, H, S> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

impl<O, H, S> Clone for OperationHandler<O, H, S>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _marker: PhantomData,
        }
    }
}

impl<O, H, S> axum::handler::Handler<(), S> for OperationHandler<O, H, S>
where
    S: Clone + Sync + Send + 'static,
    O: Operation + 'static,
    H: Handler<O, S> + Clone + Send + Sync + 'static,
    <H as Handler<O, S>>::Error: std::error::Error + Send + Sync + 'static,
    StatusCode: for<'a> From<&'a <H as Handler<O, S>>::Error>,
{
    type Future = BoxFuture<'static, RawResponse>;

    fn call(self, req: axum::extract::Request, state: S) -> Self::Future {
        async move {
            let (mut parts, body) = req.into_parts();

            let headers = parts.headers.clone();
            let token = parts.extensions.get().cloned();
            let flags = parts
                .extensions
                .get::<auth::Flags>()
                .copied()
                .expect("auth middleware set");

            match verify_auth(flags, O::AUTH) {
                Ok(_) => {}
                Err(r) => return r,
            }

            let State(state) = match State::from_request_parts(&mut parts, &state).await {
                Ok(v) => v,
                Err(_) => unreachable!("infallible"),
            };

            // Support empty body into ()
            let body = if any::TypeId::of::<O::RequestBody>() == any::TypeId::of::<()>() {
                serde_json::from_slice(b"null").expect("null is ()")
            } else {
                match Json::<O::RequestBody>::from_request(RawRequest::from_parts(parts, body), &state).await {
                    Ok(Json(body)) => body,
                    Err(e) => return error(e.status(), e),
                }
            };

            match self.handler.handle(Request { headers, body, token }, state).await {
                Ok(resp) => {
                    // Send empty body if ()
                    if any::TypeId::of::<O::ResponseBody>() == any::TypeId::of::<()>() {
                        ().into_response()
                    } else {
                        Json(resp).into_response()
                    }
                }
                Err(e) => error(StatusCode::from(&e), e),
            }
        }
        .boxed()
    }
}

// All API endpoints should return error as JSON payload
fn error(status: StatusCode, error: impl std::error::Error + Send + Sync + 'static) -> RawResponse {
    #[derive(Serialize)]
    struct Error {
        error: String,
    }

    let body = format!("{error}");

    let mut resp = (status, Json(Error { error: body })).into_response();
    resp.extensions_mut().insert(middleware::log::Error::new(error));
    resp
}

fn verify_auth(request_flags: auth::Flags, validation_flags: auth::Flags) -> Result<(), RawResponse> {
    #[derive(Debug, thiserror::Error)]
    enum Error {
        #[error("unauthenticated")]
        Unauthenticated,
        #[error("permission denied")]
        PermissionDenied,
    }

    let validation_names = auth::flag_names(validation_flags);
    let token_names = auth::flag_names(request_flags);

    // If request flags wholly contains all validation flags,
    // then user is properly authorized
    if request_flags.contains(validation_flags) {
        Ok(())
    } else if request_flags == auth::Flags::NO_AUTH {
        warn!(expected = ?validation_names, received = ?token_names, "unauthenticated");
        Err(error(StatusCode::UNAUTHORIZED, Error::Unauthenticated))
    } else {
        warn!(expected = ?validation_names, received = ?token_names, "permission denied");
        Err(error(StatusCode::FORBIDDEN, Error::PermissionDenied))
    }
}
