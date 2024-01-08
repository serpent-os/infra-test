use service::endpoint;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = endpoint::Client::connect("http://127.0.0.1:5001").await?;

    let pending = client.pending(()).await?;

    for endpoint in pending.into_inner().endpoints {
        let id = endpoint.id.unwrap();

        client.accept_pending(id).await?;
    }

    Ok(())
}
