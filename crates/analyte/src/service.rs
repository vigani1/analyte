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
    let secrets = crate::secrets::EnvironmentSecrets::load(&program)?;
    serve_with(program, space, stop, secrets, false).await
}

/// Runs synthetic localhost tests with injected host credentials and explicit URL overrides.
/// No production configuration can enable HTTP. All spawned tasks are joined.
/// # Errors
/// Reports startup, supervision and bounded shutdown failures.
pub async fn serve_loopback_test(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
    secrets: crate::secrets::EnvironmentSecrets,
) -> Result<(), Error> {
    serve_with(program, space, stop, secrets, true).await
}

async fn serve_with(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
    secrets: crate::secrets::EnvironmentSecrets,
    loopback: bool,
) -> Result<(), Error> {
    let destinations = crate::delivery::destinations(&program, loopback)?;
    let secrets = Arc::new(secrets);
    let store = Arc::new(open_store(&program, space).await?);
    crate::delivery::recover(&store).await?;
    let (delivery_stop, delivery_stopped) = watch::channel(false);
    let mut delivery_owners = JoinSet::new();
    for destination in &destinations {
        let destination = destination.clone();
        let store = store.clone();
        let secrets = secrets.clone();
        let stop = delivery_stopped.clone();
        delivery_owners.spawn(async move {
            analyte_sink::Poller::new(&store, &destination, secrets.as_ref())
                .run(stop)
                .await
        });
    }
    {
        let store = store.clone();
        let secrets = secrets.clone();
        delivery_owners.spawn(async move {
            analyte_sink::Worker::new(&store, &destinations, secrets.as_ref())
                .run_draining(delivery_stopped, crate::delivery::SHUTDOWN_BUDGET)
                .await
        });
    }
    let (shutdown, stopping) = watch::channel(false);
    let mut owners = JoinSet::new();
    let mut ready_to_close = Vec::new();
    let (close_transports, transports_closing) = watch::channel(false);
    for machine in program.machines {
        let store = store.clone();
        let stop = stopping.clone();
        let file_root = program.data_directory.join("files").join(
            analyte_contract::content_digest(machine.id.as_str().as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        );
        let (ready, received) = watch::channel(false);
        ready_to_close.push(received);
        let closing = machine::Closing {
            ready: ready.clone(),
            close: transports_closing.clone(),
        };
        owners.spawn(async move {
            let id = machine.id.clone();
            if let Err(error) = machine::run(machine, store, stop, file_root, closing).await {
                tracing::error!(machine = id.as_str(), error_kind = %error, "machine owner stopped");
            }
            ready.send_replace(true);
        });
    }
    tokio::pin!(stop);
    let result = loop {
        tokio::select! {
            result = &mut stop => break result,
            owner = delivery_owners.join_next(), if !delivery_owners.is_empty() => {
                break match owner {
                    Some(Ok(Err(error))) => Err(error.into()),
                    Some(Err(error)) => Err(error.into()),
                    Some(Ok(Ok(()))) | None => Err(invalid("delivery owner stopped unexpectedly")),
                };
            }
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
    let commits_drained = tokio::time::timeout_at(until, async {
        for ready in &mut ready_to_close {
            machine::stopping(ready).await;
        }
    })
    .await;
    delivery_stop.send_replace(true);
    let delivery_drained = tokio::time::timeout(crate::delivery::SHUTDOWN_BUDGET, async {
        while let Some(owner) = delivery_owners.join_next().await {
            owner??;
        }
        Ok::<_, Error>(())
    })
    .await;
    if !matches!(delivery_drained, Ok(Ok(()))) {
        delivery_owners.abort_all();
        while delivery_owners.join_next().await.is_some() {}
    }
    close_transports.send_replace(true);
    let drained = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(owner) = owners.join_next().await {
            owner?;
        }
        Ok::<_, Error>(())
    })
    .await;
    if !matches!(drained, Ok(Ok(()))) {
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
    commits_drained.map_err(|_| invalid("receive commits exceeded 15 seconds"))?;
    drained.map_err(|_| invalid("transport shutdown exceeded 5 seconds"))??;
    delivery_drained.map_err(|_| {
        invalid(
            "delivery shutdown exceeded 5 seconds; interrupted attempts will recover on restart",
        )
    })??;
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
