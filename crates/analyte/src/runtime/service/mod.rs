//! Starts supervised service workers and handles signals and bounded shutdown.

#[cfg(feature = "test-hooks")]
pub use super::hooks::serve_loopback_test;
use crate::runtime::Error as Failure;
use crate::runtime::Error;
use crate::{deadline, installation::config::Program, machine, runtime::space::Volume};
use analyte_store::{FreeSpace, Handle};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinSet};
#[cfg(windows)]
#[expect(unsafe_code, reason = "ADR 0017: Windows ACL FFI")]
pub mod acl;
#[cfg(windows)]
#[expect(unsafe_code, reason = "ADR 0021: Windows sign-in pipe and token FFI")]
pub(crate) mod caller;
#[cfg(test)]
mod tests;
/// Bounds pending storage work while allowing machine and operator requests.
const STORE_QUEUE_CAPACITY: usize = 32;

/// Default minimum process budget covers all ordered shutdown stages.
pub(super) const SHUTDOWN_MIN: Duration = Duration::from_secs(30);

/// Upper bound prevents an installation from waiting indefinitely on exit.
const SHUTDOWN_MAX: Duration = Duration::from_secs(300);

/// Bounds each local owner drain after admission stops.
const OWNER_SHUTDOWN_BUDGET: Duration = Duration::from_secs(5);

/// Allows admitted durable receive commits to finish before transports close.
const RECEIVE_COMMIT_BUDGET: Duration = Duration::from_secs(15);

/// Bounds offline restore and verification on the blocking owner.
const RESTORE_BUDGET: Duration = Duration::from_secs(30);

// Dropping the task also marks it stopped on panic or cancellation.
struct AdminOwner(Option<Arc<std::sync::atomic::AtomicBool>>);

impl Drop for AdminOwner {
    fn drop(&mut self) {
        if let Some(state) = &self.0 {
            state.store(false, std::sync::atomic::Ordering::Release);
        }
    }
}

/// Opens the protected store on its dedicated owner thread.
/// The installation must restrict access to the data directory and backups.
/// # Errors
/// Reports directory, migration, integrity or worker failures.
pub async fn open_store(program: &Program, space: Box<dyn FreeSpace>) -> Result<Handle, Failure> {
    open_store_with_deadline(
        program,
        space,
        Instant::now()
            .checked_add(program.store_startup_timeout())
            .ok_or_else(|| Failure::StoreStartupDeadlineOverflow)?,
    )
    .await
}

// Explicit fixture deadlines do not change the validated installation startup budget.
pub(crate) async fn open_store_with_deadline(
    program: &Program,
    space: Box<dyn FreeSpace>,
    until: Instant,
) -> Result<Handle, Failure> {
    create_data_directory(&program.data_directory)?;
    let limits = analyte_store::Limits {
        reserve_bytes: program.store_limits.reserve_bytes,
        database_bytes: program.store_limits.database_bytes,
        record_bytes: program.store_limits.record_bytes,
        page_rows: program.store_limits.page_rows,
        busy_timeout: program.store_limits.busy_timeout,
    };
    let finite = program.machines().iter().any(|m| {
        matches!(
            m.snapshot().effective().site().retention,
            analyte_config::Retention::Days(_)
        )
    });
    let key = crate::installation::secrets::retention_key(finite)?;
    let mut handle = Handle::start(
        program.data_directory.join("analyte.db"),
        limits,
        space,
        STORE_QUEUE_CAPACITY,
        until,
    )
    .await?;
    let result = handle
        .request::<_, analyte_store::MaintenanceError>(until, move |store| {
            store.configure_retention_key(key, finite)?;
            if finite {
                store.retention_space_ready()?;
            }
            Ok(())
        })
        .await;
    if let Err(error) = result {
        handle.shutdown(deadline()?).await?;
        return Err(error.into());
    }
    Ok(handle)
}

/// Runs machines in the foreground until Ctrl-C or the termination signal.
/// # Errors
/// Reports startup, signal installation or bounded shutdown failures.
pub async fn run(program: Program) -> Result<(), Error> {
    serve(program, Box::new(Volume), termination()).await
}

/// Runs with an explicit process shutdown deadline (30 through 300 seconds).
/// # Errors
/// Reports invalid bounds, startup, signal and graceful shutdown failures.
pub async fn run_bounded(program: Program, bound: Duration) -> Result<(), Error> {
    let secrets = crate::installation::secrets::Environment::load(&program)?;
    serve_bounded(program, termination(), secrets, bound, None).await
}

pub(crate) async fn serve_bounded(
    program: Program,
    stop: impl Future<Output = Result<(), Error>>,
    secrets: crate::installation::secrets::Environment,
    bound: Duration,
    watchdog: Option<Arc<crate::runtime::shutdown::Watchdog>>,
) -> Result<(), Error> {
    if !(SHUTDOWN_MIN..=SHUTDOWN_MAX).contains(&bound) {
        return Err(Failure::InvalidShutdownBound);
    }
    serve_with(
        program,
        Box::new(Volume),
        stop,
        secrets,
        Supervision {
            mode: super::hooks::ServiceMode::Production,
            shutdown_bound: bound,
            watchdog,
            ready: None,
        },
    )
    .await
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
    let secrets = crate::installation::secrets::Environment::load(&program)?;
    serve_with(
        program,
        space,
        stop,
        secrets,
        Supervision {
            mode: super::hooks::ServiceMode::Production,
            shutdown_bound: SHUTDOWN_MIN,
            watchdog: None,
            ready: None,
        },
    )
    .await
}

pub(super) struct Supervision {
    pub(super) mode: super::hooks::ServiceMode,
    pub(super) shutdown_bound: Duration,
    pub(super) watchdog: Option<Arc<crate::runtime::shutdown::Watchdog>>,
    pub(super) ready: Option<tokio::sync::oneshot::Sender<()>>,
}

pub(super) async fn serve_with(
    mut program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
    secrets: crate::installation::secrets::Environment,
    supervision: Supervision,
) -> Result<(), Error> {
    let Supervision {
        mode,
        shutdown_bound,
        watchdog,
        ready,
    } = supervision;
    let (shutdown_bound, io_budget) =
        super::hooks::shutdown_budgets(&program, mode, watchdog.as_deref(), shutdown_bound)?;
    let delivery_budget = io_budget.unwrap_or(crate::runtime::delivery::SHUTDOWN_BUDGET);
    let watchdog = match watchdog {
        Some(watchdog) => watchdog,
        None => Arc::new(crate::runtime::shutdown::Watchdog::new(
            shutdown_bound,
            None,
        )?),
    };
    let store_component = watchdog.component("store".into());
    for id in program.awaiting_setup() {
        tracing::warn!(
            machine = id.as_str(),
            "machine has no definition yet and does not run; set it up on the setup page"
        );
    }
    let destinations = Destinations::new(&program, mode, &secrets)?;
    let reach = destinations.reach();
    let secrets = Arc::new(secrets);
    let (store, admin) =
        prepare_service(&program, space, mode, &destinations, &reach, shutdown_bound).await?;
    let admin_states = admin
        .as_ref()
        .map(|server| server.owners.clone())
        .unwrap_or_default();
    let (admin_stop, admin_stopped) = watch::channel(false);
    let mut admin_owners = JoinSet::new();
    if let Some(admin) = admin {
        let component = watchdog.component("admin-server".into());
        admin_owners.spawn(async move {
            let _component = component;
            admin.run(admin_stopped).await.map_err(|error| {
                if matches!(error, crate::admin::Error::RestartRequested) {
                    Failure::RestartRequested
                } else {
                    error.into()
                }
            })
        });
    }
    let (delivery_stop, mut delivery_owners) =
        start_delivery(destinations, &store, &secrets, &watchdog, delivery_budget);
    let (shutdown, stopping) = watch::channel(false);
    let mut alert_owners = start_alerts(&program, &store, &stopping, &watchdog, reach);
    let mut machines = start_machines(&mut program, &store, &stopping, &admin_states, &watchdog);
    if let Some(ready) = ready {
        let _ = ready.send(());
    }
    let result = supervise(
        stop,
        &mut alert_owners,
        &mut admin_owners,
        &mut delivery_owners,
        &mut machines.owners,
    )
    .await;
    watchdog.arm();
    admin_stop.send_replace(true);
    super::hooks::stall(&program, &store).await?;
    shutdown.send_replace(true);
    Draining {
        alert_owners,
        admin_owners,
        delivery_owners,
        owners: machines.owners,
        ready_to_close: machines.ready_to_close,
        delivery_stop,
        close_transports: machines.close_transports,
        captures: machines.captures,
        delivery_budget,
        capture_budget: io_budget.unwrap_or(machine::capture::SHUTDOWN_BUDGET),
        store_budget: io_budget.unwrap_or(OWNER_SHUTDOWN_BUDGET),
    }
    .finish(store, store_component)
    .await?;
    result
}

// Startup owns the store until all fallible preparations have completed.
async fn prepare_service(
    program: &Program,
    space: Box<dyn FreeSpace>,
    mode: crate::runtime::hooks::ServiceMode,
    destinations: &Destinations,
    reach: &crate::operator::alerts::Reach,
    shutdown: Duration,
) -> Result<(Arc<Handle>, Option<crate::admin::Server>), Error> {
    let store = open_service_store(program, space).await?;
    let startup = async {
        super::hooks::publish_store_probes(program, &store)?;
        let paused = store
            .request(deadline()?, |store| store.restore_unreconciled())
            .await?;
        tracing::info!(
            order_sending_paused_after_restore = paused,
            "store restore reconciliation status"
        );
        crate::runtime::delivery::recover(&store).await?;
        crate::runtime::delivery::report_unconfigured(&store, &reach.configured).await?;
        super::recovery::run(program, &store).await?;
        crate::admin::prepare(
            program,
            store.clone(),
            mode,
            &destinations.draining,
            shutdown,
        )
        .await
        .map_err(admin_startup_error)
    }
    .await;
    let admin = match startup {
        Ok(admin) => admin,
        Err(error) => {
            close_store(store, OWNER_SHUTDOWN_BUDGET).await?;
            return Err(error);
        }
    };
    Ok((store, admin))
}

struct Machines {
    captures: Vec<Arc<machine::capture::Capture>>,
    owners: JoinSet<()>,
    ready_to_close: Vec<watch::Receiver<bool>>,
    close_transports: watch::Sender<bool>,
}

fn start_machines(
    program: &mut Program,
    store: &Arc<Handle>,
    stopping: &watch::Receiver<bool>,
    admin_states: &std::collections::BTreeMap<String, Arc<std::sync::atomic::AtomicBool>>,
    watchdog: &crate::runtime::shutdown::Watchdog,
) -> Machines {
    let mut owners = JoinSet::new();
    let mut captures = Vec::new();
    let mut ready_to_close = Vec::new();
    let (close_transports, transports_closing) = watch::channel(false);
    for machine in std::mem::take(&mut program.machines) {
        let capture = machine::capture::Capture::new(
            &machine,
            &program.data_directory,
            program.store_limits.reserve_bytes,
        );
        let capture = capture.map(Arc::new);
        if let Some(owner) = &capture {
            captures.push(owner.clone());
        }
        let admin_state = AdminOwner(admin_states.get(machine.id.as_str()).cloned());
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
        let component = watchdog.component(format!("machine:{}", machine.id.as_str()));
        owners.spawn(async move {
            let _admin_state = admin_state;
            let _component = component;
            let id = machine.id.clone();
            if let Err(error) = machine::run(machine, store, stop, file_root, closing, capture).await {
                tracing::error!(machine = id.as_str(), error_kind = %error, "machine owner stopped");
            }
            ready.send_replace(true);
        });
    }
    Machines {
        captures,
        owners,
        ready_to_close,
        close_transports,
    }
}

struct Draining {
    delivery_budget: Duration,
    capture_budget: Duration,
    store_budget: Duration,
    captures: Vec<Arc<machine::capture::Capture>>,
    alert_owners: JoinSet<Result<(), Error>>,
    admin_owners: JoinSet<Result<(), Error>>,
    delivery_owners: JoinSet<Result<DeliveryExit, DeliveryOwnerError>>,
    owners: JoinSet<()>,
    ready_to_close: Vec<watch::Receiver<bool>>,
    delivery_stop: watch::Sender<bool>,
    close_transports: watch::Sender<bool>,
}

impl Draining {
    async fn finish(
        self,
        store: Arc<Handle>,
        store_component: crate::runtime::shutdown::Component,
    ) -> Result<(), Error> {
        let Self {
            mut alert_owners,
            mut admin_owners,
            mut delivery_owners,
            mut owners,
            mut ready_to_close,
            delivery_stop,
            close_transports,
            captures,
            delivery_budget,
            capture_budget,
            store_budget,
        } = self;
        let until = tokio::time::Instant::now()
            .checked_add(RECEIVE_COMMIT_BUDGET)
            .unwrap_or(tokio::time::Instant::now());
        // Admin reads drain alongside machine commits, without adding a new stage
        // to the process shutdown budget or delaying machine admission shutdown.
        let (alerts_drained, admin_drained, commits_drained) = tokio::join!(
            tokio::time::timeout(OWNER_SHUTDOWN_BUDGET, async {
                while let Some(owner) = alert_owners.join_next().await {
                    owner??;
                }
                Ok::<_, Error>(())
            }),
            tokio::time::timeout(OWNER_SHUTDOWN_BUDGET, async {
                while let Some(owner) = admin_owners.join_next().await {
                    owner??;
                }
                Ok::<_, Error>(())
            }),
            tokio::time::timeout_at(until, async {
                for ready in &mut ready_to_close {
                    let _ = async {
                        let _ = ready.wait_for(|&stopped| stopped).await;
                    }
                    .await;
                }
            })
        );
        if !matches!(alerts_drained, Ok(Ok(()))) {
            alert_owners.abort_all();
            while alert_owners.join_next().await.is_some() {}
        }
        if !matches!(admin_drained, Ok(Ok(()))) {
            admin_owners.abort_all();
            while admin_owners.join_next().await.is_some() {}
        }
        delivery_stop.send_replace(true);
        let delivery_drained = tokio::time::timeout(delivery_budget, async {
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
        let drained = tokio::time::timeout(OWNER_SHUTDOWN_BUDGET, async {
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
        for capture in captures {
            let mut capture =
                Arc::try_unwrap(capture).map_err(|_| Failure::CaptureOwnershipRetained)?;
            capture.close(capture_budget).await;
        }
        close_store(store, store_budget).await?;
        drop(store_component);
        alerts_drained.map_err(Failure::AlertShutdownTimeout)??;
        admin_drained.map_err(Failure::AdminShutdownExceeded5Seconds)??;
        commits_drained.map_err(Failure::ReceiveCommitsExceeded15Seconds)?;
        drained.map_err(Failure::TransportShutdownExceeded5Seconds)??;
        delivery_drained.map_err(Failure::DeliveryShutdownTimeout)??;
        Ok(())
    }
}

async fn close_store(store: Arc<Handle>, budget: Duration) -> Result<(), Error> {
    let mut store = Arc::try_unwrap(store).map_err(|_| Failure::StoreOwnershipRetained)?;
    store
        .shutdown(Instant::now().checked_add(budget).unwrap_or(Instant::now()))
        .await?;
    drop(store);
    Ok(())
}

/// Why a delivery owner ended without an error.
enum DeliveryExit {
    /// Shutdown was requested.
    Stopped,
    /// A kept destination's worker sent everything; only these end while running.
    Delivered,
}

/// Clears a kept destination's "still delivering" flag when its worker ends,
/// also on an error, a panic or an abort.
struct Delivering(Arc<std::sync::atomic::AtomicBool>);

impl Drop for Delivering {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// The destinations delivery workers run for.
struct Destinations {
    /// Used by set-up machines: a delivery worker and an order poller each.
    served: Vec<analyte_sink::Destination>,
    /// Kept after their machines were removed: a delivery worker only, until empty.
    retired: Vec<analyte_sink::Destination>,
    /// Each retired destination's "still delivering" flag, shown by the admin page.
    draining: crate::admin::Draining,
}

impl Destinations {
    fn new(
        program: &Program,
        mode: super::hooks::ServiceMode,
        secrets: &crate::installation::secrets::Environment,
    ) -> Result<Self, Error> {
        let retired = crate::runtime::delivery::unused_lab_apps(program, mode)?;
        Ok(Self {
            served: crate::runtime::delivery::destinations(program, mode)?,
            // Each kept destination's worker clears its flag when nothing is left to
            // send. Only a kept destination can lack keys: a set-up machine's missing
            // keys stop startup instead.
            draining: program
                .unused_lab_apps()
                .map(|lab_app| {
                    let settings = lab_app.settings();
                    let kept = crate::admin::Kept {
                        delivering: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                        keys_missing: !secrets.has_keys(&settings.name),
                        api_key: settings.api_key_name.clone(),
                        signing_key: settings.signing_key_name.clone(),
                    };
                    (settings.name.as_str().to_owned(), kept)
                })
                .collect(),
            retired,
        })
    }

    /// What the `undeliverable_destination` alert compares stored items with:
    /// the same missing keys the admin page reports, decided once at startup.
    fn reach(&self) -> crate::operator::alerts::Reach {
        crate::operator::alerts::Reach {
            software: Default::default(),
            configured: self
                .served
                .iter()
                .chain(&self.retired)
                .map(|destination| destination.id().clone())
                .collect(),
            keyless: self
                .retired
                .iter()
                .filter(|destination| {
                    self.draining
                        .get(destination.id().as_str())
                        .is_some_and(|kept| kept.keys_missing)
                })
                .map(|destination| destination.id().clone())
                .collect(),
        }
    }
}

/// Supervised owners combine exactly polling and app delivery boundaries.
#[derive(Debug, thiserror::Error)]
enum DeliveryOwnerError {
    #[error(transparent)]
    Poll(#[from] analyte_sink::PollError),
    #[error(transparent)]
    Delivery(#[from] analyte_sink::DeliveryError),
}

fn start_delivery(
    destinations: Destinations,
    store: &Arc<Handle>,
    secrets: &Arc<crate::installation::secrets::Environment>,
    watchdog: &crate::runtime::shutdown::Watchdog,
    delivery_budget: Duration,
) -> (
    watch::Sender<bool>,
    JoinSet<Result<DeliveryExit, DeliveryOwnerError>>,
) {
    let (delivery_stop, delivery_stopped) = watch::channel(false);
    let mut delivery_owners = JoinSet::new();
    for destination in &destinations.served {
        let destination = destination.clone();
        let store = store.clone();
        let secrets = secrets.clone();
        let stop = delivery_stopped.clone();
        let component = watchdog.component(format!("order-poller:{}", destination.id().as_str()));
        delivery_owners.spawn(async move {
            let _component = component;
            analyte_sink::Poller::new(&store, &destination, secrets.as_ref())
                .run(stop)
                .await
                .map(|()| DeliveryExit::Stopped)
                .map_err(DeliveryOwnerError::from)
        });
    }
    for destination in destinations.served {
        let store = store.clone();
        let secrets = secrets.clone();
        let stop = delivery_stopped.clone();
        let component =
            watchdog.component(format!("delivery-worker:{}", destination.id().as_str()));
        delivery_owners.spawn(async move {
            let _component = component;
            analyte_sink::Worker::for_destination(&store, &destination, secrets.as_ref())
                .run_draining(stop, delivery_budget)
                .await
                .map(|()| DeliveryExit::Stopped)
                .map_err(DeliveryOwnerError::from)
        });
    }
    for destination in destinations.retired {
        let store = store.clone();
        let secrets = secrets.clone();
        let stop = delivery_stopped.clone();
        let flag = destinations
            .draining
            .get(destination.id().as_str())
            .map(|kept| kept.delivering.clone())
            .unwrap_or_default();
        let component = watchdog.component(format!(
            "retired-delivery-worker:{}",
            destination.id().as_str()
        ));
        delivery_owners.spawn(async move {
            let _component = component;
            let _delivering = Delivering(flag);
            let stopping = stop.clone();
            analyte_sink::Worker::for_destination(&store, &destination, secrets.as_ref())
                .run_until_delivered(stop, delivery_budget)
                .await
                .map_err(DeliveryOwnerError::from)?;
            Ok(if *stopping.borrow() {
                DeliveryExit::Stopped
            } else {
                tracing::info!(
                    destination = destination.id().as_str(),
                    "retired lab app destination has nothing left to deliver; its worker stopped"
                );
                DeliveryExit::Delivered
            })
        });
    }
    (delivery_stop, delivery_owners)
}

fn start_alerts(
    program: &Program,
    store: &Arc<Handle>,
    stopping: &watch::Receiver<bool>,
    watchdog: &crate::runtime::shutdown::Watchdog,
    reach: crate::operator::alerts::Reach,
) -> JoinSet<Result<(), Error>> {
    let mut alert_owners = JoinSet::new();
    for endpoint in program.alert_delivery().endpoints() {
        let store = store.clone();
        let endpoint = endpoint.clone();
        let policy = program.alert_delivery().clone();
        let stop = stopping.clone();
        let component = watchdog.component(format!("alert-endpoint-{}", endpoint.id().as_str()));
        alert_owners.spawn(async move {
            let _component = component;
            crate::runtime::alert_delivery::run(store, endpoint, policy, stop).await
        });
    }

    {
        let store = store.clone();
        let policies: Vec<_> = crate::runtime::retention::policies(program)
            .into_iter()
            .filter(|(_, days)| days.is_some())
            .collect();
        let stop = stopping.clone();
        let component = watchdog.component("retention-worker".into());
        if !policies.is_empty() {
            alert_owners.spawn(async move {
                let _component = component;
                crate::runtime::retention::run(store, policies, stop).await
            });
        }
    }
    if !program.alerts().is_empty() {
        let store = store.clone();
        let policies = program.alerts().to_vec();
        let mut reach = reach;
        reach.software = program
            .machines()
            .iter()
            .filter_map(|machine| {
                let check = machine.snapshot.effective().machine().software_version()?;
                Some((machine.id().clone(), check.revision))
            })
            .collect();
        let interval = program.alert_interval();
        let destinations = program
            .machines()
            .iter()
            .filter_map(|machine| {
                let destination = machine.snapshot.effective().site().destination.as_ref()?;
                Some((
                    destination.name.clone(),
                    machine.destination_limits.freshness.as_secs(),
                ))
            })
            .collect();
        let stop = stopping.clone();
        let component = watchdog.component("alert-evaluator".into());
        alert_owners.spawn(async move {
            let _component = component;
            crate::operator::alerts::run(store, policies, destinations, reach, interval, stop)
                .await
                .map_err(Into::into)
        });
    }
    alert_owners
}

async fn supervise(
    stop: impl Future<Output = Result<(), Error>>,
    alert_owners: &mut JoinSet<Result<(), Error>>,
    admin_owners: &mut JoinSet<Result<(), Error>>,
    delivery_owners: &mut JoinSet<Result<DeliveryExit, DeliveryOwnerError>>,
    owners: &mut JoinSet<()>,
) -> Result<(), Error> {
    tokio::pin!(stop);
    loop {
        tokio::select! {
            result = &mut stop => break result,
            owner = alert_owners.join_next(), if !alert_owners.is_empty() => {
                break match owner {
                    Some(Ok(Err(error))) => Err(error),
                    Some(Err(error)) => Err(error.into()),
                    Some(Ok(Ok(()))) | None => Err(Failure::AlertEvaluatorStoppedUnexpectedly),
                }
            }
            owner = admin_owners.join_next(), if !admin_owners.is_empty() => {
                break match owner {
                    Some(Ok(Err(error))) => Err(error),
                    Some(Err(error)) => Err(error.into()),
                    Some(Ok(Ok(()))) | None => Err(Failure::AdminOwnerStoppedUnexpectedly),
                }
            }
            owner = delivery_owners.join_next(), if !delivery_owners.is_empty() => {
                break match owner {
                    Some(Ok(Ok(DeliveryExit::Delivered))) => continue,
                    Some(Ok(Err(error))) => Err(error.into()),
                    Some(Err(error)) => Err(error.into()),
                    Some(Ok(Ok(DeliveryExit::Stopped))) | None => {
                        Err(Failure::DeliveryOwnerStoppedUnexpectedly)
                    }
                }
            }
            owner = owners.join_next(), if !owners.is_empty() => {
                if let Some(Err(error)) = owner {
                    tracing::error!(error_kind = %error, "machine supervisor observed task failure");
                }
            }
        }
    }
}

async fn termination() -> Result<(), Error> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        tokio::select! {
            signal = interrupt.recv() => signal.ok_or_else(|| Failure::SigintSignalStreamClosed)?,
            signal = term.recv() => signal.ok_or_else(|| Failure::SigtermSignalStreamClosed)?,
        }
    }
    #[cfg(windows)]
    {
        let mut term = tokio::signal::windows::ctrl_break()?;
        tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = term.recv() => {} }
    }
    Ok(())
}

pub(crate) fn create_data_directory(path: &std::path::Path) -> Result<(), Failure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        if std::fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(Failure::InsecureDataDirectory);
        }
    }
    #[cfg(windows)]
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        acl::create_private(path, acl::Readers::Operators)?;
        // Like Unix, an existing non-directory entry is an I/O error.
        std::fs::create_dir_all(path)?;
        acl::check(path, acl::Readers::Operators)?;
    }
    Ok(())
}

/// Restores offline on a blocking owner, keeping the displaced store.
/// # Errors
/// Refuses active owners, invalid backups and storage failures.
pub async fn restore(
    program: Program,
    source: std::path::PathBuf,
    audit: analyte_store::Audit,
) -> Result<std::path::PathBuf, Error> {
    Ok(tokio::task::spawn_blocking(move || {
        analyte_store::Store::restore(
            &program.data_directory.join("analyte.db"),
            &source,
            analyte_store::Limits {
                reserve_bytes: program.store_limits.reserve_bytes,
                database_bytes: program.store_limits.database_bytes,
                record_bytes: program.store_limits.record_bytes,
                page_rows: program.store_limits.page_rows,
                busy_timeout: program.store_limits.busy_timeout,
            },
            Box::new(Volume),
            &audit,
            Instant::now()
                .checked_add(RESTORE_BUDGET)
                .unwrap_or(Instant::now()),
        )
    })
    .await??)
}

// Shared ownership begins only in orchestration; preserve its original I/O failure identity.
async fn open_service_store(
    program: &Program,
    space: Box<dyn FreeSpace>,
) -> Result<Arc<Handle>, Error> {
    let store = open_store(program, space).await?;
    Ok(Arc::new(store))
}

// The service's existing startup API exposes original dependency causes to recovery callers.
fn admin_startup_error(error: crate::admin::Error) -> Error {
    Error::from(error)
}

impl From<DeliveryOwnerError> for Error {
    fn from(error: DeliveryOwnerError) -> Self {
        match error {
            DeliveryOwnerError::Poll(error) => Self::SinkPollError(error),
            DeliveryOwnerError::Delivery(error) => Self::SinkDeliveryError(error),
        }
    }
}
