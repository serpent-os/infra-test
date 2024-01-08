use std::error::Error;

use futures::{future::BoxFuture, FutureExt};
use log::{debug, error};
use tonic::{body::BoxBody, transport::Body};
use tower::BoxError;

pub fn log_error<T>(
    result: Result<tonic::Response<T>, tonic::Status>,
) -> Result<tonic::Response<T>, tonic::Status> {
    match result {
        Ok(resp) => Ok(resp),
        Err(err) => {
            if let Some(source) = err.source() {
                error!("Handler error, {source}");
            }
            Err(err)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Log;

impl<S> tower::Layer<S> for Log {
    type Service = Service<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Service { inner }
    }
}

#[derive(Debug, Clone)]
pub struct Service<S> {
    inner: S,
}

impl<S> tower::Service<http::Request<Body>> for Service<S>
where
    S: tower::Service<http::Request<Body>, Response = http::Response<BoxBody>, Error = BoxError>
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

        async move {
            debug!("Request received: {req:?}");

            match inner.call(req).await {
                Ok(resp) => {
                    debug!("Response: {resp:?}");
                    Ok(resp)
                }
                // Infallible
                Err(e) => Err(e),
            }
        }
        .boxed()
    }
}
