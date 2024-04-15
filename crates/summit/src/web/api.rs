use std::{fmt, sync::Arc};

use axum::{
    response::{IntoResponse, Response},
    routing::get,
    Json,
};
use color_eyre::eyre;
use serde::Serialize;
use serde_json::json;
use service::State;

mod endpoints;

pub fn router() -> axum::Router<State> {
    axum::Router::new()
        .route(
            "/status",
            get(|| async {
                Body::ok(json!({
                    "status": "ok",
                }))
            }),
        )
        .route("/endpoints", get(endpoints::get))
        .fallback(|| async { Error::message("unknown method") })
}

pub enum Body<T> {
    Success { data: T },
    Error { error: Error },
}

impl<T> Body<T> {
    pub fn ok(data: T) -> Self {
        Self::Success { data }
    }

    pub fn error(error: impl Into<Error>) -> Self {
        Self::Error { error: error.into() }
    }
}

impl<T> IntoResponse for Body<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match self {
            Body::Success { data } => Json(json!({
                "success": true,
                "data": data,
            }))
            .into_response(),
            Body::Error { error } => {
                let mut response = Json(json!({
                    "success": false,
                    // Default format only returns top-most error.
                    // We can use `.context` to provide a friendly error
                    // to return here, then when logging the error internally
                    // we can use alternate # display to log the causes as well
                    "error": format!("{}", error.0),
                }))
                .into_response();
                response.extensions_mut().insert(error);
                response
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Error(Arc<eyre::Error>);

impl Error {
    pub fn message(message: impl ToString) -> Self {
        Self(Arc::new(eyre::format_err!(message.to_string())))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<E> From<E> for Error
where
    E: Into<eyre::Error>,
{
    fn from(source: E) -> Self {
        Self(Arc::new(source.into()))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        Body::<()>::error(self).into_response()
    }
}
