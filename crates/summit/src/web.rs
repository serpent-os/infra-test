use std::{
    future::{Future, IntoFuture},
    io,
};

use axum::{extract::Request, middleware::Next, response::Response, Router};
use log::{debug, error};
use service::State;
use tokio::net::ToSocketAddrs;

use crate::Result;

mod api;

pub async fn serve(
    address: impl ToSocketAddrs,
    state: State,
) -> Result<impl Future<Output = Result<(), io::Error>> + Send> {
    let app = Router::new()
        .nest("/api/v1", api::router())
        .layer(axum::middleware::from_fn(log))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address).await?;
    Ok(axum::serve(listener, app).into_future())
}

async fn log(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    debug!("{method} {path}: {request:?}");

    let response = next.run(request).await;

    if let Some(error) = response.extensions().get::<api::Error>() {
        // # alternate format will log causes
        error!("{method} {path}: {error:#}");
    } else {
        debug!("{method} {path}: {response:?}");
    }

    response
}
