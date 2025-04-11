use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
};
use color_eyre::eyre::{self, Context};
use http::StatusCode;
use thiserror::Error;

use crate::task;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

pub async fn tasks(State(state): State<service::State>) -> Result<Html<&'static str>, Error> {
    // TODO: Serialize tasks
    let mut conn = state.service_db.acquire().await.context("acquire db conn")?;

    // TODO: Add pagination & sorting to query params
    let _tasks = task::query(&mut conn, task::query::Params::default())
        .await
        .context("query tasks")?;

    // TODO: Render template
    Ok(Html(include_str!("../templates/index.html")))
}

pub async fn fallback() -> Html<&'static str> {
    Html(include_str!("../templates/404.html"))
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct Error(#[from] eyre::Report);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        // TODO: Error page template?
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    }
}
