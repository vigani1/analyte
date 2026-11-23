//! Owns machine workers and protocol sessions; links own transport session loops.
pub(crate) mod astm;
pub(crate) mod autolyser;
mod driver;
pub(crate) mod link;
pub(crate) use link::capture;
use link::{folder, serial, tcp};
pub(crate) mod hl7;
pub(crate) mod interpretation;
pub use interpretation::reprocess;
pub(crate) mod law;
pub(crate) mod poct;
pub(crate) mod protocols;
pub(crate) mod receive;
pub(crate) use receive::connection_input;
pub(crate) mod vidas;
mod worker;
use crate::installation::config::Machine;
use analyte_config::TcpRole;
use analyte_store::Handle;
use analyte_transport::Limits;
use protocols::{Error, Link};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
pub(crate) mod hooks;

/// Bounds queued transport events and writes per machine connection.
const TRANSPORT_QUEUE_CAPACITY: usize = 16;

/// Bounds the transport close stage after receive commits finish.
const TRANSPORT_CLOSE_BUDGET: Duration = Duration::from_secs(25);

/// Separates completed receive commits from transport closure during shutdown.
pub(crate) struct Closing {
    pub(crate) ready: watch::Sender<bool>,
    pub(crate) close: watch::Receiver<bool>,
}

impl Closing {
    pub(crate) async fn after_commits(&mut self) -> Result<(), link::error::Error> {
        self.ready.send_replace(true);
        let _ = tokio::time::timeout(
            TRANSPORT_CLOSE_BUDGET,
            self.close.wait_for(|&stopped| stopped),
        )
        .await
        .map_err(link::error::Error::TransportCloseStageExceeded)?;
        Ok(())
    }
}

/// Independent-link LAW listens for inbound transactions whatever its configured role.
pub(crate) fn independent_law(stack: &analyte_config::Stack) -> Option<&analyte_config::LawStack> {
    if let analyte_config::Stack::Law(settings) = stack
        && matches!(
            settings.policy().links(),
            analyte_config::LawLinks::Independent { .. }
        )
    {
        return Some(settings);
    }
    None
}

/// Runs one machine until stop; its link light reads "not listening/watching" after.
/// An aborted owner task leaves the light at its last state; the admin page's owner
/// row then says "stopped", so the light is never read alone.
pub(crate) async fn run(
    machine: Machine,
    store: Arc<Handle>,
    stop: watch::Receiver<bool>,
    file_root: std::path::PathBuf,
    closing: Closing,
    capture: Option<Arc<capture::Capture>>,
) -> Result<(), Error> {
    let status = machine.status.clone();
    let result = serve(machine, store, stop, file_root, closing, capture).await;
    status.ready(false);
    result
}

async fn serve(
    mut machine: Machine,
    store: Arc<Handle>,
    stop: watch::Receiver<bool>,
    file_root: std::path::PathBuf,
    closing: Closing,
    capture: Option<Arc<capture::Capture>>,
) -> Result<(), Error> {
    if let Some(settings) = independent_law(machine.snapshot.effective().stack()) {
        let settings = Arc::new(settings.clone());
        return Ok(law::independent::run(machine, settings, store, stop, closing, capture).await?);
    }
    let listener = machine.listener.take();
    let link = protocols::transport(machine.snapshot.effective().stack());
    let timeout = Duration::from_millis(u64::from(link.timeout_ms()));
    let limits = Limits::new(TRANSPORT_QUEUE_CAPACITY, timeout)?;
    let capture = capture.as_deref();
    match link {
        Link::Tcp {
            role: TcpRole::Server,
            session,
            ..
        } => {
            tcp::server(
                listener, &machine, &store, session, limits, stop, closing, capture,
            )
            .await
        }
        Link::Tcp {
            role: TcpRole::Client,
            session,
            ..
        } => tcp::client(&machine, &store, session, limits, stop, closing, capture).await,
        Link::Serial(link) => {
            serial::run(&machine, &store, link, limits, stop, closing, capture).await
        }
        Link::Folder { bytes, .. } => {
            folder::session(&machine, &store, bytes, file_root, stop, closing, capture)
                .await
                .map_err(Into::into)
        }
    }
}

pub(in crate::machine) async fn drive(
    machine: &Machine,
    selected: protocols::StreamSession<'_>,
    store: &Handle,
    mut conn: analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    closing: &mut Closing,
) -> Result<(), Error> {
    let result = protocols::session(machine, selected, store, &mut conn, stop.clone()).await;

    if machine.hooks.enabled(hooks::Pause::ConnectionClosing) {
        machine
            .hooks
            .pause(hooks::Pause::ConnectionClosing, &mut stop.clone())
            .await
            .map_err(Error::TestHook)?;
    }
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    conn.cancellation().cancel();
    // Drop closes the event consumer so a transport ending after an error cannot
    // wait behind unread events during shutdown.
    result
}
pub(crate) mod software_version;
