//! Host-owned independent LAW links, TF-2b B.6 pp.79-80.
use super::{Connection, Error, evidence, invalid};
use crate::{config::Machine, machine::Closing};
use analyte_config::{LawLinks, LawStack};
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
    pub(super) orders: Connection,
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
    store: Arc<Handle>,
    mut stop: watch::Receiver<bool>,
    mut closing: Closing,
    capture: Option<crate::capture::Capture>,
) -> Result<(), Error> {
    let analyte_config::Stack::Law(settings) = machine.snapshot.effective().stack() else {
        return Err(invalid("independent LAW stack absent"));
    };
    let settings = Arc::new(settings.as_ref().clone());
    let listener = match machine.listener.take() {
        Some(listener) => TcpListener::from_std(listener)?,
        None => {
            analyte_transport::tcp::bind(machine.snapshot.effective().site().endpoint.parse()?)
                .await?
        }
    };
    let machine = Arc::new(machine);
    let capture = capture.map(Arc::new);
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
        ()=crate::machine::stopping(&mut stop)=>(Ok(()),Finished::Stop),
        result=&mut inbound=>(result,Finished::Inbound),
        result=&mut outbound=>(result,Finished::Outbound),
    };
    shutdown.send_replace(true);
    let cleanup = match side {
        Finished::Stop => {
            let (inbound, outbound) = tokio::join!(&mut inbound, &mut outbound);
            inbound.and(outbound)
        }
        Finished::Inbound => outbound.await,
        Finished::Outbound => inbound.await,
    };
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    result.and(cleanup)
}

async fn server(
    listener: TcpListener,
    machine: Arc<Machine>,
    store: Arc<Handle>,
    settings: Arc<LawStack>,
    outbound: mpsc::Sender<Work>,
    mut stop: watch::Receiver<bool>,
    capture: Option<Arc<crate::capture::Capture>>,
) -> Result<(), Error> {
    let mut sessions = JoinSet::new();
    let (sessions_stop, sessions_rx) = watch::channel(false);
    let mut result=async {
        loop {
            tokio::select! {
                biased;
                ()=crate::machine::stopping(&mut stop)=>break,
                completed=sessions.join_next(),if !sessions.is_empty()=>{
                    if let Some(completed)=completed { report(&machine,completed.map_err(|error|task_failure(&error))?); }
                },
                accepted=listener.accept(),if sessions.len()<SESSION_LIMIT=>{
                    let (stream,_)=accepted?;
                    sessions.spawn(accepted_connection(
                        stream, machine.clone(), store.clone(), settings.clone(),
                        outbound.clone(), sessions_rx.clone(), capture.clone(),
                    ));
                }
            }
        }
        Ok::<_,Error>(())
    }.await;
    sessions_stop.send_replace(true);
    while let Some(completed) = sessions.join_next().await {
        match completed {
            Ok(completed) => report(&machine, completed),
            Err(error) => {
                tracing::error!(
                    machine = machine.id.as_str(),
                    task_panicked = error.is_panic(),
                    "LAW inbound task did not return an outcome during shutdown"
                );
                if result.is_ok() {
                    result = Err(task_failure(&error));
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
    capture: Option<Arc<crate::capture::Capture>>,
) -> Result<(), Error> {
    let limits = analyte_transport::Limits::new(
        CHANNEL_LIMIT,
        Duration::from_millis(u64::from(settings.transport().operation_timeout_ms())),
    )?;
    let (mut conn, side) = analyte_transport::connection(limits);
    let side = match &capture {
        Some(capture) => capture.attach(side),
        None => side,
    };
    let context = evidence::Connection::new(&machine)?;
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
    let (result, transport) = tokio::join!(dialog, analyte_transport::tcp::pump(stream, side));
    transport?;
    result
}

// Join diagnostics must not expose a panic payload from a source transaction.
fn task_failure(error: &tokio::task::JoinError) -> Error {
    if error.is_panic() {
        invalid("LAW inbound task panicked; machine owner stopped")
    } else {
        invalid("LAW inbound task was cancelled before returning its outcome")
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
    capture: Option<Arc<crate::capture::Capture>>,
) -> Result<(), Error> {
    let mut idle_orders = Connection::new()?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let result=async {
        loop {
            tokio::select! {
                biased;
                ()=crate::machine::stopping(&mut stop)=>break,
                job=queue.recv()=>{
                    let Some(mut job)=job else {break;};
                    let until=job.orders.due().ok_or("LAW queued work has no deadline")?;
                    let ready=tokio::select! {
                        biased;
                        ()=crate::machine::stopping(&mut stop)=>Err("LAW-independent-shutdown-before-response"),
                        ready=tokio::time::timeout_at(until.into(),&mut job.ready)=>match ready {
                            Ok(Ok(()))=>Ok(()),
                            Ok(Err(_))=>Err("LAW-independent-query-response-issuance-uncertain"),
                            Err(_)=>Err("LAW-independent-queue-deadline"),
                        },
                    };
                    match ready {
                        Ok(())=>{
                            let result=exchange(&machine,&store,&settings,&mut job.orders,&job.message,stop.clone(),capture.as_deref()).await;
                            settle(&machine,&store,&mut job.orders,result).await?;
                        }
                        Err(reason)=>job.orders.close(&store,reason).await?,
                    }
                },
                _=tick.tick()=>{
                    let context=evidence::Connection::new(&machine)?;
                    let outgoing=tokio::select! {biased; ()=crate::machine::stopping(&mut stop)=>break, result=idle_orders.idle(&store,&context,settings.options())=>result};
                    match outgoing {
                        Ok(Some(message))=>{let result=exchange(&machine,&store,&settings,&mut idle_orders,&message,stop.clone(),capture.as_deref()).await;settle(&machine,&store,&mut idle_orders,result).await?;},
                        Ok(None)=>{},
                        Err(error)=>{idle_orders.close(&store,"LAW-independent-admission-failed").await?;report(&machine,Err(error));},
                    }
                }
            }
        }
        Ok::<_,Error>(())
    }.await;
    queue.close();
    while let Ok(mut job) = queue.try_recv() {
        job.orders.close(&store, "LAW-independent-shutdown").await?;
    }
    idle_orders
        .close(&store, "LAW-independent-shutdown")
        .await?;
    result
}

// Persist the specific failure beside the uncertain reservation, not just in diagnostics.
async fn settle(
    machine: &Machine,
    store: &Handle,
    orders: &mut Connection,
    result: Result<(), Error>,
) -> Result<(), Error> {
    let reason = result.as_ref().err().map_or_else(
        || "LAW-independent-exchange-ended".to_owned(),
        |error| format!("LAW-independent-failure:{error}"),
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
    orders: &mut Connection,
    message: &Message,
    mut stop: watch::Receiver<bool>,
    capture: Option<&crate::capture::Capture>,
) -> Result<(), Error> {
    let LawLinks::Independent { outbound } = settings.policy().links() else {
        return Err(invalid("LAW independent destination absent"));
    };
    let until = orders.due().ok_or("LAW outbound deadline absent")?;
    if std::time::Instant::now() >= until {
        return Err(invalid("LAW outbound work expired before connection"));
    }
    let stream = tokio::select! {
        biased;
        ()=crate::machine::stopping(&mut stop)=>return Ok(()),
        result=tokio::time::timeout_at(until.into(),TcpStream::connect(outbound))=>result.map_err(|_|invalid("LAW outbound connect deadline"))??,
    };
    let limits = analyte_transport::Limits::new(
        CHANNEL_LIMIT,
        Duration::from_millis(u64::from(settings.transport().operation_timeout_ms())),
    )?;
    let (mut conn, side) = analyte_transport::connection(limits);
    let side = match capture {
        Some(capture) => capture.attach(side),
        None => side,
    };
    let context = evidence::Connection::new(machine)?;
    let dialog = async move {
        let result = transaction(
            &context, store, settings, orders, message, &mut conn, &mut stop,
        )
        .await;
        conn.cancellation().cancel();
        drop(conn);
        result
    };
    let (result, transport) = tokio::join!(dialog, analyte_transport::tcp::pump(stream, side));
    transport?;
    result
}

async fn transaction(
    machine: &evidence::Connection<'_>,
    store: &Handle,
    settings: &LawStack,
    orders: &mut Connection,
    message: &Message,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut host = super::Host::new(
        machine,
        settings,
        store,
        conn,
        stop.clone(),
        orders,
        analyte_dialog::law::Link::WorkResponse,
    )?;
    let result = host.outbound(message).await;
    host.finish().await?;
    result
}
