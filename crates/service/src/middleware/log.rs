//! Log the request and if applicable, error

use std::sync::Arc;

use axum::body::Body;
use futures::{future::BoxFuture, FutureExt};
use tracing::{debug, error, info_span, Instrument};

use crate::error;

/// Logging middleware which logs the request and if applicable, error
#[derive(Debug, Clone, Copy)]
pub struct Log;

impl<S> tower::Layer<S> for Log {
    type Service = Service<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Service { inner }
    }
}

/// Tower service of the [`Log`] layer
#[derive(Debug, Clone)]
pub struct Service<S> {
    inner: S,
}

impl<S> tower::Service<http::Request<Body>> for Service<S>
where
    S: tower::Service<http::Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        tower::Service::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let path = req.uri().path().to_string();

        async move {
            debug!("Request received");

            match inner.call(req).await {
                Ok(resp) => {
                    let (parts, body) = resp.into_parts();

                    if let Some(Error(e)) = parts.extensions.get() {
                        let error = error::chain(e);
                        error!(%error, "Handler error");
                    }

                    let resp = http::Response::from_parts(parts, body);

                    debug!(status = %resp.status(), "Sending response");

                    Ok(resp)
                }
                Err(e) => Err(e),
            }
        }
        .instrument(info_span!("request", path))
        .boxed()
    }
}

/// If set as a response extension, it will be logged by this middleware
#[derive(Clone)]
pub struct Error(Arc<dyn std::error::Error + Send + Sync>);

impl Error {
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Arc::new(error))
    }
}
