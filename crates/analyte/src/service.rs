//! Service ownership, signal handling and bounded shutdown.
use crate::{Error, config::Program, deadline, invalid, machine, space::Volume};
use analyte_store::{FreeSpace, Handle};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinSet};

/// Opens the protected store on its dedicated owner thread.
/// The installation must restrict access to the data directory and backups.
/// # Errors
/// Reports directory, migration, integrity or worker failures.
pub async fn open_store(program: &Program, space: Box<dyn FreeSpace>) -> Result<Handle, Error> {
    create_data_directory(&program.data_directory)?;
    let limits = analyte_store::Limits {
        reserve_bytes: program.store_limits.reserve_bytes,
        database_bytes: program.store_limits.database_bytes,
        record_bytes: program.store_limits.record_bytes,
        page_rows: program.store_limits.page_rows,
        busy_timeout: program.store_limits.busy_timeout,
    };
    Ok(Handle::start(
        program.data_directory.join("analyte.db"),
        limits,
        space,
        32,
        deadline(),
    )
    .await?)
}

/// Runs machines in the foreground until Ctrl-C or the termination signal.
/// # Errors
/// Reports startup, signal installation or bounded shutdown failures.
pub async fn run(program: Program) -> Result<(), Error> {
    serve(program, Box::new(Volume), termination()).await
}

/// Runs the receive service with an injectable disk probe and shutdown signal.
/// Machine failures are reported independently; they never cancel another machine.
/// Shutdown stops admission, lets submitted commits finish, closes transports, then
/// closes the store. Cancellation may leave stored evidence requiring reconciliation.
/// # Errors
/// Reports store startup or shutdown failure and supervision task panics.
pub async fn serve(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
) -> Result<(), Error> {
    let store = Arc::new(open_store(&program, space).await?);
    let (shutdown, stopping) = watch::channel(false);
    let mut owners = JoinSet::new();
    for machine in program.machines {
        let store = store.clone();
        let stop = stopping.clone();
        let file_root = program.data_directory.join("files").join(
            analyte_contract::content_digest(machine.id.as_str().as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        );
        owners.spawn(async move {
            let id = machine.id.clone();
            if let Err(error) = machine::run(machine, store, stop, file_root).await {
                tracing::error!(machine = id.as_str(), error_kind = %error, "machine owner stopped");
            }
        });
    }
    // Delivery-worker and order-poller startup belongs here. This receive-only
    // release deliberately creates no deliveries and issues no machine orders.
    tokio::pin!(stop);
    let result = loop {
        tokio::select! {
            result = &mut stop => break result,
            owner = owners.join_next(), if !owners.is_empty() => {
                if let Some(Err(error)) = owner {
                    tracing::error!(error_kind = %error, "machine supervisor observed task failure");
                }
            }
        }
    };
    shutdown.send_replace(true);
    let until = tokio::time::Instant::now()
        .checked_add(Duration::from_secs(15))
        .unwrap_or(tokio::time::Instant::now());
    let drained = tokio::time::timeout_at(until, async {
        while let Some(owner) = owners.join_next().await {
            owner?;
        }
        Ok::<_, Error>(())
    })
    .await;
    if drained.is_err() {
        owners.abort_all();
        while owners.join_next().await.is_some() {}
    }
    let mut store = Arc::try_unwrap(store)
        .map_err(|_| invalid("machine retained store ownership after shutdown"))?;
    store
        .shutdown(
            Instant::now()
                .checked_add(Duration::from_secs(5))
                .unwrap_or(Instant::now()),
        )
        .await?;
    drained.map_err(|_| invalid("machine shutdown exceeded 15 seconds"))??;
    result
}

async fn termination() -> Result<(), Error> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = term.recv() => {} }
    }
    #[cfg(windows)]
    {
        let mut term = tokio::signal::windows::ctrl_break()?;
        tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = term.recv() => {} }
    }
    Ok(())
}

fn create_data_directory(path: &std::path::Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        if std::fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(invalid("data directory must deny group and other access"));
        }
    }
    #[cfg(windows)]
    std::fs::create_dir_all(path)?;
    Ok(())
}
