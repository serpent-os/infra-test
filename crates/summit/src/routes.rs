use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Response},
};
use color_eyre::eyre::{self, Context};
use http::StatusCode;
use serde::Deserialize;
use thiserror::Error;

use crate::{project, task};

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

#[derive(Debug, Deserialize)]
pub struct TasksQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

pub async fn tasks(
    State(state): State<service::State>,
    Query(query): Query<TasksQuery>,
) -> Result<Html<&'static str>, Error> {
    const DEFAULT_LIMIT: u32 = 25;
    const MAX_LIMIT: u32 = 100;

    let mut conn = state.service_db.acquire().await.context("acquire db conn")?;

    let limit = query.per_page.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let offset = query.page.unwrap_or(0) as i64 * limit as i64;

    let _projects = project::list(&mut conn).await.context("list projects")?;
    let query = task::query(&mut conn, task::query::Params::default().offset(offset).limit(limit))
        .await
        .context("query tasks")?;

    dbg!((query.count, query.total));

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
