use auth::authenticate;
use clap::Parser;
use tonic::{transport::Server, Request, Response, Status};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args { host, port } = Args::parse();

    let address = format!("{host}:{port}");

    println!("[info] auth server listening on {address}");

    Server::builder()
        .add_service(auth::Server::with_interceptor(Service, interceptor))
        .serve(address.parse()?)
        .await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(default_value = "127.0.0.1")]
    host: String,
    #[arg(long, short, default_value = "5001")]
    port: u16,
}

struct Service;

#[tonic::async_trait]
impl auth::Service for Service {
    async fn authenticate(
        &self,
        request: Request<authenticate::Request>,
    ) -> Result<Response<authenticate::Response>, Status> {
        const PASSWORD: &str = "superdupersecretpassword";
        const TOKEN: &str = "superdupersecrettoken";

        let authenticate::Request { username, password } = request.into_inner();

        if password == PASSWORD {
            println!("[info] {username} authenticated");

            Ok(Response::new(authenticate::Response {
                token: TOKEN.to_string(),
            }))
        } else {
            println!("[info] {username} invalid credentials");

            Err(Status::unauthenticated("invalid credentials"))
        }
    }
}

// TODO: Actual logging
fn interceptor(request: Request<()>) -> Result<Request<()>, Status> {
    println!("[debug] Request received: {:?}", &request);
    Ok(request)
}
