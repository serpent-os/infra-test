use axum::extract;
use color_eyre::eyre::Context;
use serde::Serialize;
use service::{Endpoint, State};
use tracing::debug;

use super::{Body, Error};

pub async fn get(extract::State(state): extract::State<State>) -> Result<Body<Endpoints>, Error> {
    debug!("Some event from the handler");

    let endpoints = Endpoint::list(&state.db)
        .await
        .context("failed to list endpoints")?;

    Ok(Body::ok(Endpoints { endpoints }))
}

#[derive(Serialize)]
pub struct Endpoints {
    pub endpoints: Vec<Endpoint>,
}
