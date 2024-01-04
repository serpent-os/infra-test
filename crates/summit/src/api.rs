use axum::{routing::get, Json};
use serde::Serialize;

pub fn router() -> axum::Router {
    axum::Router::new().route(
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
}
