use auth::authenticate;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

pub async fn authenticate(
    State(mut auth_client): State<auth::Client>,
    Json(payload): Json<AuthenticateBody>,
) -> Result<Json<AuthenticateResponse>, StatusCode> {
    let AuthenticateBody { username, password } = payload;

    let result = auth_client
        .authenticate(authenticate::Request {
            username: username.clone(),
            password,
        })
        .await;

    match result {
        Ok(response) => {
            println!("[info] {username} authenticated");

            let authenticate::Response { token } = response.into_inner();

            Ok(Json(AuthenticateResponse { token }))
        }
        Err(status) => {
            println!(
                "[error] authentication failed for {username}: {}",
                status.message()
            );

            match status.code() {
                tonic::Code::Unauthenticated => Err(StatusCode::UNAUTHORIZED),
                _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthenticateBody {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthenticateResponse {
    token: String,
}
