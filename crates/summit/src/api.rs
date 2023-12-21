use axum::routing::post;

pub mod account;

pub fn router(auth_client: auth::Client) -> axum::Router {
    axum::Router::new()
        .route("/account/authenticate", post(account::authenticate))
        .with_state(auth_client)
}
