use std::{
    future::{Future, IntoFuture},
    io,
    path::Path,
};

use axum::{extract::Request, middleware::Next, response::Response, Router};
use service::State;
use tokio::net::ToSocketAddrs;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{debug, error};

use crate::Result;

mod api;

pub async fn serve(
    address: impl ToSocketAddrs,
    assets: impl AsRef<Path>,
    state: State,
) -> Result<impl Future<Output = Result<(), io::Error>> + Send> {
    let static_dir =
        ServeDir::new(&assets).not_found_service(ServeFile::new(assets.as_ref().join("404.html")));

    let app = Router::new()
        .nest("/api/v1", api::router())
        .fallback_service(static_dir)
        .layer(axum::middleware::from_fn(log))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address).await?;
    Ok(axum::serve(listener, app).into_future())
}

#[tracing::instrument(
    name = "http",
    skip_all,
    fields(
        method = %request.method(), 
        path = %request.uri().path(),
    )
)]
async fn log(request: Request, next: Next) -> Response {
    debug!(headers = ?request.headers(), "Request received");

    let response = next.run(request).await;

    if let Some(error) = response.extensions().get::<api::Error>() {
        // # alternate format will log causes
        error!(error = format!("{error:#}"), "Error handling request");
    } else {
        debug!(status = %response.status(), "Sending response");
    }

    response
}
