use axum::{routing::get, Json};
use serde::Serialize;
use service::State;

mod endpoints;

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
        .route("/endpoints", get(endpoints::get))
}
