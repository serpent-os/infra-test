use axum::{extract, routing::get, Json};
use log::error;
use serde::Serialize;
use service::{Endpoint, State};

pub fn router() -> axum::Router<State> {
    axum::Router::new()
        .route(
            "/status",
            get(|| async {
                #[derive(Serialize)]
                struct Status {
                    status: String,
                }

                Json(Status {
                    status: "ok".into(),
                })
            }),
        )
        .route(
            "/endpoints",
            get(|extract::State(state): extract::State<State>| async move {
                #[derive(Serialize)]
                struct Endpoints {
                    endpoints: Vec<Endpoint>,
                }

                let endpoints = Endpoint::list(&state.db)
                    .await
                    .map_err(|e| error!("List endpoint failed: {e}"))
                    .unwrap_or_default();

                Json(Endpoints { endpoints })
            }),
        )
}
