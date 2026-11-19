//! Drives independent LAW connections with host-owned I/O (TF-2b B.6).
use super::{Error, LawOrderLink, intake};
use crate::machine::hooks::Pause;
use crate::machine::link::{self, Kind, Next, session_ended};
use crate::{installation::config::Machine, machine::Closing};
use analyte_config::{LawLinks, LawStack};
use analyte_dialog::law::outcome::{Code as LawEnd, Detail as LawDetail};
use analyte_hl7::Message;
use analyte_store::Handle;
use std::{sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

// Bounded handoff transfers reservation ownership; it never grants resend permission.
pub(super) struct Work {
    pub(super) orders: LawOrderLink,
    pub(super) message: Message,
    pub(super) ready: oneshot::Receiver<()>,
}

enum Finished {
    Stop,
    Inbound,
    Outbound,
}

pub(in crate::machine) async fn run(
    mut machine: Machine,
    settings: Arc<LawStack>,
    store: Arc<Handle>,
    mut stop: watch::Receiver<bool>,
    mut closing: Closing,
    capture: Option<Arc<crate::machine::capture::Capture>>,
) -> Result<(), Error> {
    let listener = match machine.listener.take() {
        Some(listener) => TcpListener::from_std(listener)?,
        None => analyte_transport::tcp::bind(link::socket_address(&machine)?).await?,
    };
    machine.status.ready(true);
    let machine = Arc::new(machine);
    let (sender, receiver) = mpsc::channel(8);
    let (shutdown, local_stop) = watch::channel(false);
    let inbound = server(
        listener,
        machine.clone(),
        store.clone(),
        settings.clone(),
        sender,
        local_stop.clone(),
        capture.clone(),
    );
    let outbound = worker(machine, store, settings, receiver, local_stop, capture);
    tokio::pin!(inbound, outbound);
    // Every task and queued reservation is joined/settled on each exit path.
    let (result, side) = tokio::select! {
        biased;
        _=async { let _ = stop.wait_for(|&stopped| stopped).await; }=>(Ok(()),Finished::Stop),
        result=&mut inbound=>(result,Finished::Inbound),
        result=&mut outbound=>(result,Finished::Outbound),
    };
    shutdown.send_replace(true);
    let cleanup = match side {
        Finished::Stop => {
            let (inbound, outbound) = tokio::join!(&mut inbound, &mut outbound);
            let mut result = inbound;
            crate::machine::receive::custody::retain_cleanup(&mut result, outbound);
            result
        }
        Finished::Inbound => outbound.await,
        Finished::Outbound => inbound.await,
    };
    let mut result = result;
    crate::machine::receive::custody::retain_cleanup(&mut result, cleanup);
    if *stop.borrow() {
        let retained = closing.after_commits().await;
        crate::machine::receive::custody::retain_cleanup(&mut result, retained.map_err(Into::into));
    }
    result
}

async fn server(
    listener: TcpListener,
    machine: Arc<Machine>,
    store: Arc<Handle>,
    settings: Arc<LawStack>,
    outbound: mpsc::Sender<Work>,
    mut stop: watch::Receiver<bool>,
    capture: Option<Arc<crate::machine::capture::Capture>>,
) -> Result<(), Error> {
    let mut sessions = JoinSet::new();
    let (sessions_stop, sessions_rx) = watch::channel(false);
    let mut accepting = stop.clone();
    let mut result = async {
        loop {
            tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => break,
                completed = sessions.join_next(), if !sessions.is_empty() => {
                    if let Some(completed) = completed {
                        // A panicked or cancelled task left its dialog in an unknown state;
                        // that is an owner fault, never a served session.
                        let (result, transport) = completed.map_err(task_failure)?;
                        let id = machine.id.as_str();
                        match session_ended(id, Kind::LawInbound, result, transport, &mut stop).await {
                            Next::Serve => {}
                            Next::Stop | Next::StopIdle => break,
                        }
                    }
                },
                accepted = link::tcp::accept(|| listener.accept(), machine.id.as_str(), &mut accepting),
                    if sessions.len() < SESSION_LIMIT => {
                    let stream = match accepted {
                        Ok(Some((stream, _))) => stream,
                        Ok(None) => break,
                        // accept retries every recoverable fault; only an unusable listener returns.
                        Err(error) => return Err(error.into()),
                    };
                    sessions.spawn(accepted_connection(
                        stream,
                        machine.clone(),
                        store.clone(),
                        settings.clone(),
                        outbound.clone(),
                        sessions_rx.clone(),
                        capture.clone(),
                    ));
                }
            }
        }
        Ok::<_, Error>(())
    }
    .await;
    sessions_stop.send_replace(true);
    while let Some(completed) = sessions.join_next().await {
        match completed {
            Ok((result, transport)) => {
                link::report(machine.id.as_str(), Kind::LawInbound, result, transport);
            }
            Err(error) => {
                tracing::error!(
                    machine = machine.id.as_str(),
                    task_panicked = error.is_panic(),
                    "LAW inbound task did not return an outcome during shutdown"
                );
                if result.is_ok() {
                    result = Err(task_failure(error));
                }
            }
        }
    }
    result
}
// Bound concurrent inbound dialogs; excess connections remain in the listener backlog.
const SESSION_LIMIT: usize = 4;
// Bound queued transport events and commands per link, independently of dialog admission.
const CHANNEL_LIMIT: usize = 16;

async fn accepted_connection(
    stream: TcpStream,
    machine: Arc<Machine>,
    store: Arc<Handle>,
    settings: Arc<LawStack>,
    outbound: mpsc::Sender<Work>,
    stop: watch::Receiver<bool>,
    capture: Option<Arc<crate::machine::capture::Capture>>,
) -> (Result<(), Error>, Result<(), analyte_transport::Error>) {
    let limits = match analyte_transport::Limits::new(
        CHANNEL_LIMIT,
        Duration::from_millis(u64::from(
            settings.settings().transport().operation_timeout_ms(),
        )),
    ) {
        Ok(limits) => limits,
        Err(error) => return (Err(error.into()), Ok(())),
    };
    let (mut conn, side) = link::recorded_connection(limits, capture.as_deref(), &machine.status);
    let context = match intake::ConnectionEvidence::new(&machine) {
        Ok(context) => context,
        Err(error) => return (Err(error.into()), Ok(())),
    };
    let dialog = async move {
        let result = super::session_with_outbound(
            &context,
            &settings,
            &store,
            &mut conn,
            stop,
            Some(&outbound),
        )
        .await;
        conn.cancellation().cancel();
        drop(conn);
        result
    };
    tokio::join!(dialog, analyte_transport::tcp::pump(stream, side))
}

// Join diagnostics must not expose a panic payload from a source transaction.
fn task_failure(error: tokio::task::JoinError) -> Error {
    if error.is_panic() {
        Error::InboundTaskPanicked(error)
    } else {
        Error::InboundTaskCancelled(error)
    }
}
fn report(machine: &Machine, result: Result<(), Error>) {
    if let Err(error) = result {
        tracing::error!(machine=machine.id.as_str(),error_kind=%error,"independent LAW transaction failed; no work retried");
    }
}

async fn worker(
    machine: Arc<Machine>,
    store: Arc<Handle>,
    settings: Arc<LawStack>,
    mut queue: mpsc::Receiver<Work>,
    mut stop: watch::Receiver<bool>,
    capture: Option<Arc<crate::machine::capture::Capture>>,
) -> Result<(), Error> {
    let mut idle_orders = LawOrderLink::new()?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let result = async {
        loop {
            tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => break,
                job = queue.recv() => {
                    let Some(mut job) = job else { break; };

                    if machine.hooks.enabled(Pause::LawQueued)
                        && let Err(error) = machine.hooks.pause(Pause::LawQueued, &mut stop).await {
                            job.orders.close(&store, "LAW-test-queue-gate-stopped").await?;
                            return Err(Error::from(error));
                    }
                    let until = job.orders.due().ok_or(Error::QueuedWorkHasNoDeadline)?;
                    let until = machine.hooks.queued_deadline(until);
                    let ready = tokio::select! {
                        biased;
                        _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => {
                            Err(LawEnd::ResponseShutdown.as_str())
                        },
                        ready = tokio::time::timeout_at(until.into(), &mut job.ready) => match ready {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(_)) => Err(LawEnd::QueryIssuanceUncertain.as_str()),
                            Err(_) => Err(LawEnd::IndependentQueueDeadline.as_str()),
                        },
                    };
                    match ready {
                        Ok(()) => {
                            let result = exchange(
                                &machine,
                                &store,
                                &settings,
                                &mut job.orders,
                                &job.message,
                                stop.clone(),
                                capture.as_deref(),
                            ).await;
                            settle(&machine, &store, &mut job.orders, result).await?;
                        }
                        Err(reason) => job.orders.close(&store, reason).await?,
                    }
                },
                _ = tick.tick() => {
                    let context = intake::ConnectionEvidence::new(&machine)?;
                    let outgoing = tokio::select! {
                        biased;
                        _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => break,
                        result = idle_orders.idle(&store, context.machine(), &settings) => result,
                    };
                    match outgoing {
                        Ok(Some(message)) => {
                            let result = exchange(
                                &machine,
                                &store,
                                &settings,
                                &mut idle_orders,
                                &message,
                                stop.clone(),
                                capture.as_deref(),
                            ).await;
                            settle(&machine, &store, &mut idle_orders, result).await?;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            idle_orders.close(
                                &store,
                                LawEnd::IndependentAdmissionFailed.as_str(),
                            ).await?;
                            report(&machine, Err(error.into()));
                        }
                    }
                }
            }
        }
        Ok::<_, Error>(())
    }
    .await;
    queue.close();
    while let Ok(mut job) = queue.try_recv() {
        job.orders
            .close(&store, LawEnd::IndependentShutdown.as_str())
            .await?;
    }
    idle_orders
        .close(&store, LawEnd::IndependentShutdown.as_str())
        .await?;
    result
}

// Persist the specific failure beside the uncertain reservation, not just in diagnostics.
async fn settle(
    machine: &Machine,
    store: &Handle,
    orders: &mut LawOrderLink,
    result: Result<(), Error>,
) -> Result<(), Error> {
    let reason = result.as_ref().err().map_or_else(
        || LawEnd::IndependentExchangeEnded.as_str().to_owned(),
        |error| LawDetail::IndependentFailure(error.to_string()).to_string(),
    );
    orders.close(store, &reason).await?;
    report(machine, result);
    Ok(())
}

// One dial, one issuance and one response. Cancellation/failure is uncertain, never retried.
async fn exchange(
    machine: &Machine,
    store: &Handle,
    settings: &LawStack,
    orders: &mut LawOrderLink,
    message: &Message,
    mut stop: watch::Receiver<bool>,
    capture: Option<&crate::machine::capture::Capture>,
) -> Result<(), Error> {
    let LawLinks::Independent { outbound } = settings.policy().links() else {
        return Err(Error::IndependentDestinationAbsent);
    };
    let until = orders.due().ok_or(Error::OutboundDeadlineAbsent)?;
    if std::time::Instant::now() >= until {
        return Err(Error::OutboundWorkExpired);
    }
    let stream = tokio::select! {
        biased;
        _=async { let _ = stop.wait_for(|&stopped| stopped).await; }=>return Ok(()),
        result=tokio::time::timeout_at(until.into(),TcpStream::connect(outbound))=>result.map_err(Error::OutboundConnectDeadline)??,
    };
    let limits = analyte_transport::Limits::new(
        CHANNEL_LIMIT,
        Duration::from_millis(u64::from(
            settings.settings().transport().operation_timeout_ms(),
        )),
    )?;
    let (mut conn, side) = link::recorded_connection(limits, capture, &machine.status);
    let context = intake::ConnectionEvidence::new(machine)?;
    let dialog = async move {
        let result = transaction(
            &context, store, settings, orders, message, &mut conn, &mut stop,
        )
        .await;
        conn.cancellation().cancel();
        drop(conn);
        result
    };
    let (result, pumped) = tokio::join!(dialog, analyte_transport::tcp::pump(stream, side));
    // One exchange, never retried: the transport failure takes precedence, as before.
    pumped.map_err(Error::from).and(result)
}

async fn transaction(
    machine: &intake::ConnectionEvidence<'_>,
    store: &Handle,
    settings: &LawStack,
    orders: &mut LawOrderLink,
    message: &Message,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), Error> {
    super::driver::run(
        || {
            super::Host::open(
                machine,
                settings,
                store,
                orders,
                super::Mode::Outbound(message),
            )
        },
        super::Context {
            conn,
            stop: stop.clone(),
        },
    )
    .await
}
