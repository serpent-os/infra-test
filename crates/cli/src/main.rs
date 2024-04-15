use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::Result;
use service::{account, crypto::KeyPair, endpoint};
use tonic::transport::Uri;

#[tokio::main]
async fn main() -> Result<()> {
    let Args { private_key } = Args::parse();

    let key_pair = KeyPair::load(private_key)?;

    println!("Using key_pair {}", key_pair.public_key().encode());

    let summit_uri: Uri = "http://127.0.0.1:5001".parse()?;

    let tokens = account::service::authenticate(summit_uri.clone(), "admin".to_string(), key_pair).await?;

    let mut client = endpoint::service::connect_with_auth(summit_uri, tokens.api_token).await?;

    let pending = client.pending(()).await?;

    for endpoint in pending.into_inner().endpoints {
        let id = endpoint.id.unwrap();

        client.accept_pending(id).await?;
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(help = "Path to admin private key")]
    private_key: PathBuf,
}
