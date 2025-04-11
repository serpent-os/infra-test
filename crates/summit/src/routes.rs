use axum::response::Html;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

pub async fn fallback() -> Html<&'static str> {
    Html(include_str!("../templates/404.html"))
}
