//! Owns machine workers and transport sessions without replaying queued writes.
use crate::machine::error::Error;
mod astm;
pub(crate) mod autolyser;
mod driver;
pub(crate) mod link;
pub(crate) use link::capture;
use link::{folder, serial};
pub(crate) mod hl7;
pub(crate) mod interpretation;
pub use interpretation::reprocess;
pub(crate) mod law;
mod poct;
pub(crate) mod receive;
mod settings;
pub(crate) use receive::{connection_input, custody};
pub(crate) mod vidas;
mod worker;
use crate::installation::config::Machine;
use analyte_config::TcpRole;
use analyte_store::Handle;
use analyte_transport::{Backoff, Limits, connection};
use settings::Link;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// Bounds queued transport events and writes per machine connection.
const TRANSPORT_QUEUE_CAPACITY: usize = 16;

/// Bounds the transport close stage after receive commits finish.
const TRANSPORT_CLOSE_BUDGET: Duration = Duration::from_secs(25);

/// Caps reconnection churn while keeping cancellation responsive.
const RECONNECT_MIN: Duration = Duration::from_millis(100);

/// Caps exponential transport reconnection backoff.
const RECONNECT_MAX: Duration = Duration::from_secs(5);

/// Separates completed receive commits from transport closure during shutdown.
pub(crate) struct Closing {
    pub(crate) ready: watch::Sender<bool>,
    pub(crate) close: watch::Receiver<bool>,
}

impl Closing {
    pub(crate) async fn after_commits(&mut self) -> Result<(), link::error::Error> {
        self.ready.send_replace(true);
        tokio::time::timeout(TRANSPORT_CLOSE_BUDGET, stopping(&mut self.close))
            .await
            .map_err(link::error::Error::TransportCloseStageExceeded)?;
        Ok(())
    }
}

pub(crate) async fn run(
    mut machine: Machine,
    store: Arc<Handle>,
    mut stop: watch::Receiver<bool>,
    file_root: std::path::PathBuf,
    mut closing: Closing,
    capture: Option<Arc<capture::Capture>>,
) -> Result<(), Error> {
    if let analyte_config::Stack::Law(settings) = machine.snapshot.effective().stack()
        && matches!(
            settings.policy().links(),
            analyte_config::LawLinks::Independent { .. }
        )
    {
        return Ok(law::independent::run(machine, store, stop, closing, capture).await?);
    }
    let link = settings::transport(machine.snapshot.effective().stack());
    let timeout = Duration::from_millis(u64::from(link.timeout_ms()));
    let limits = Limits::new(TRANSPORT_QUEUE_CAPACITY, timeout)?;
    let endpoint = &machine.snapshot.effective().site().endpoint;
    match link {
        Link::Tcp {
            role: TcpRole::Server,
            session,
            ..
        } => {
            let listener = match machine.listener.take() {
                Some(listener) => tokio::net::TcpListener::from_std(listener)?,
                None => analyte_transport::tcp::bind(endpoint.parse()?).await?,
            };
            loop {
                let accepted = tokio::select! {
                    biased;
                    _ = stop.changed() => { closing.after_commits().await?; return Ok(()); },
                    accepted = listener.accept() => accepted?,
                };
                let (conn, side) = recorded_connection(limits, capture.as_deref());
                let cancel = conn.cancellation();
                let processing = drive(&machine, session, &store, conn, stop.clone(), &mut closing);
                let transport = analyte_transport::tcp::pump(accepted.0, side);
                // Drain ordered received/closed events even if the I/O owner finishes first.
                let (result, transport) = tokio::join!(processing, transport);
                cancel.cancel();
                transport?;
                if let Err(error) = result {
                    tracing::error!(machine = machine.id.as_str(), error_kind = %error, "machine session stopped");
                }
                if *stop.borrow() {
                    return Ok(());
                }
            }
        }
        Link::Tcp {
            role: TcpRole::Client,
            session,
            ..
        } => loop {
            let (conn, side) = recorded_connection(limits, capture.as_deref());
            let cancel = conn.cancellation();
            let transport =
                analyte_transport::tcp::client_session(endpoint.parse()?, side, retry()?);
            let (result, transport) = tokio::join!(
                drive(&machine, session, &store, conn, stop.clone(), &mut closing),
                transport
            );
            // drive cancels the I/O owner and drops its consumer after session cleanup.
            // The client's final Closed event can therefore meet a closed queue; that
            // expected owner shutdown must not terminate persistent reconnection.
            if let Err(error) = transport
                && !(matches!(error, analyte_transport::Error::ConsumerClosed)
                    && cancel.is_cancelled())
            {
                return Err(error.into());
            }
            if let Err(error) = result {
                tracing::error!(machine=machine.id.as_str(),error_kind=%error,"machine session stopped");
            }
            if *stop.borrow() {
                return Ok(());
            }
            // Reconnect backoff is cancellable; this is not polling for completion.
            tokio::select! {
                () = stopping(&mut stop) => return Ok(()),
                () = tokio::time::sleep(RECONNECT_MIN) => {}
            }
        },
        Link::Serial(link) => {
            serial::run(
                &machine,
                &store,
                link,
                limits,
                stop,
                closing,
                capture.as_deref(),
            )
            .await
        }
        Link::Folder { bytes, .. } => folder::session(
            &machine,
            &store,
            bytes,
            file_root,
            stop,
            closing,
            capture.as_deref(),
        )
        .await
        .map_err(Into::into),
    }
}

fn recorded_connection(
    limits: Limits,
    capture: Option<&capture::Capture>,
) -> (analyte_transport::Connection, analyte_transport::IoSide) {
    let (conn, side) = connection(limits);
    let side = match capture {
        Some(capture) => capture.attach(side),
        None => side,
    };
    (conn, side)
}

fn retry() -> Result<Backoff, Error> {
    Ok(Backoff::new(RECONNECT_MIN, RECONNECT_MAX, 1)?)
}

async fn session(
    machine: &Machine,
    selected: &analyte_config::Stack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let context = crate::intake::Connection::new(machine)?;
    let machine = &context;
    match selected {
        analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(settings)) => {
            astm::session(machine, settings, store, conn, stop)
                .await
                .map_err(Into::into)
        }
        analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(settings)) => {
            hl7::session(machine, settings, store, conn, stop)
                .await
                .map_err(Into::into)
        }
        analyte_config::Stack::Law(settings) => law::session(machine, settings, store, conn, stop)
            .await
            .map_err(Into::into),
        analyte_config::Stack::Poct(settings) => {
            poct::session(machine, settings, store, conn, stop)
                .await
                .map_err(Into::into)
        }
        analyte_config::Stack::Vidas(settings) => {
            vidas::session(machine, settings, store, conn, stop)
                .await
                .map_err(Into::into)
        }
        analyte_config::Stack::Autolyser(settings) => {
            autolyser::session(machine, settings, store, conn, stop)
                .await
                .map_err(Into::into)
        }
        analyte_config::Stack::Astm(analyte_config::RecordTransport::Folder(_))
        | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => {
            Err(Error::FolderStream)
        }
    }
}

pub(crate) async fn stopping(stop: &mut watch::Receiver<bool>) {
    if *stop.borrow() {
        return;
    }
    let _ = stop.changed().await;
}

async fn drive(
    machine: &Machine,
    selected: &analyte_config::Stack,
    store: &Handle,
    mut conn: analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    closing: &mut Closing,
) -> Result<(), Error> {
    let result = session(machine, selected, store, &mut conn, stop.clone()).await;
    #[cfg(feature = "test-hooks")]
    if let Some(hook) = &machine.hooks.connection_closing {
        hook.pause(&mut stop.clone())
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

pub(crate) mod error;
