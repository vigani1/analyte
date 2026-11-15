//! Loopback reference receiver; use a TLS reverse proxy for a remote deployment.
#![forbid(unsafe_code)]
use clap::Parser;
#[derive(Parser)]
struct Arguments {
    #[arg(long, default_value = "receiver.sqlite")]
    database: std::path::PathBuf,
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: std::net::SocketAddr,
    #[arg(long)]
    seed: Option<std::path::PathBuf>,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse();
    if !arguments.listen.ip().is_loopback() {
        return Err("reference binary must bind loopback".into());
    }
    let key = std::env::var("ANALYTE_API_KEY")?;
    let secret = analyte_contract::SigningSecret::parse(&std::env::var("ANALYTE_SIGNING_SECRET")?)?;
    let receiver = analyte_receiver::Receiver::open(&arguments.database, key, secret)?;
    if let Some(path) = arguments.seed {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        receiver.seed(analyte_contract::decode(&bytes)?).await?;
    }
    let listener = tokio::net::TcpListener::bind(arguments.listen).await?;
    axum::serve(listener, receiver.router())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    receiver.shutdown().map_err(|_| "storage worker panicked")?;
    Ok(())
}
