//! Runs a loopback reference receiver for integration tests.
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse();
    if !arguments.listen.ip().is_loopback() {
        return Err("reference binary must bind loopback".into());
    }
    let key = std::env::var("ANALYTE_API_KEY")?;
    let secret = analyte_contract::SigningSecret::parse(&std::env::var("ANALYTE_SIGNING_SECRET")?)?;
    let mut receiver = analyte_receiver::Receiver::open(&arguments.database, key, secret)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        if let Some(path) = arguments.seed {
            use std::io::Read as _;
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(1_048_577)
                .read_to_end(&mut bytes)?;
            receiver.seed(analyte_contract::decode(&bytes)?).await?;
        }
        let listener = tokio::net::TcpListener::bind(arguments.listen).await?;
        // Report the address only after seeding and binding have succeeded, including port zero.
        {
            use std::io::Write as _;
            let mut output = std::io::stdout().lock();
            writeln!(output, "{}", listener.local_addr()?)?;
            output.flush()?;
        }
        axum::serve(listener, receiver.router())
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    });
    let cleanup = runtime.block_on(receiver.shutdown());
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(original), Err(cleanup)) => Err(Box::new(CleanupFailure { original, cleanup })),
    }
}
#[derive(Debug, thiserror::Error)]
#[error("{original}; receiver cleanup failed: {cleanup}")]
struct CleanupFailure {
    #[source]
    original: Box<dyn std::error::Error>,
    cleanup: analyte_receiver::ReceiverError,
}
