use axum::{extract, Json};
use log::error;
use serde::Serialize;
use service::{Endpoint, State};

pub async fn get(extract::State(state): extract::State<State>) -> Json<Endpoints> {
    let endpoints = Endpoint::list(&state.db)
        .await
        .map_err(|error| error!("List endpoints failed: {error}"))
        .unwrap_or_default();

    Json(Endpoints { endpoints })
}

#[derive(Serialize)]
pub struct Endpoints {
    pub endpoints: Vec<Endpoint>,
}
