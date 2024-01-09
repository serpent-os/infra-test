use std::{
    future::{Future, IntoFuture},
    io,
};

use axum::{extract::Request, middleware::Next, response::Response, Router};
use log::debug;
use tokio::net::ToSocketAddrs;

use crate::Result;

mod api;

pub async fn serve(
    address: impl ToSocketAddrs,
) -> Result<impl Future<Output = Result<(), io::Error>> + Send> {
    let app = Router::new()
        .nest("/api", api::router())
        .layer(axum::middleware::from_fn(log));
    let listener = tokio::net::TcpListener::bind(address).await?;
    Ok(axum::serve(listener, app).into_future())
}

async fn log(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    debug!("Received request for {path}");

    let response = next.run(request).await;

    debug!("Returning response for {path}: {}", response.status());

    response
}
