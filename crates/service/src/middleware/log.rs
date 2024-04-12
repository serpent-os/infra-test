use futures::{future::BoxFuture, FutureExt};
use itertools::Itertools;
use tonic::{body::BoxBody, transport::Body};
use tower::BoxError;
use tracing::{debug, error, info_span, Instrument};

pub fn log_handler<T, E>(result: Result<T, E>) -> Result<tonic::Response<T>, tonic::Status>
where
    E: Into<tonic::Status> + std::error::Error,
{
    match result {
        Ok(data) => Ok(tonic::Response::new(data)),
        Err(err) => {
            // Log the chain of errors below the tonic::Status
            // Our handler errors should convert to tonic::Status
            // and set themselves as the source.
            let mut chain = vec![err.to_string()];
            let mut source = err.source();
            while let Some(cause) = source {
                chain.push(cause.to_string());
                source = cause.source();
            }
            let error = chain.into_iter().join(": ");
            error!(%error, "Handler error");

            Err(err.into())
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

        let path = req.uri().path().to_string();

        async move {
            debug!("Request received");

            match inner.call(req).await {
                Ok(resp) => {
                    debug!(status = %resp.status(), "Sending response");
                    Ok(resp)
                }
                // Infallible
                Err(e) => Err(e),
            }
        }
        .instrument(info_span!("grpc", path))
        .boxed()
    }
}
