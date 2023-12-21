use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    Router,
};
use clap::Parser;
use url::Url;

mod api;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args { host, port, auth } = Args::parse();

    let auth_client = auth::Client::connect(auth.to_string()).await?;

    let app = Router::new()
        .nest("/api", api::router(auth_client))
        .layer(middleware::from_fn(log));

    let address = format!("{host}:{port}");

    println!("[info] summit listening on {address}");

    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: String,
    #[arg(long, short, default_value = "5000")]
    port: u16,
    #[arg(long, default_value = "http://127.0.0.1:5001")]
    auth: Url,
}

async fn log(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    println!("[debug] Received request for {path}");

    let response = next.run(request).await;

    println!(
        "[debug] Returning response for {path}: {}",
        response.status()
    );

    response
}
