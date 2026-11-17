//! Owns machine workers and transport sessions without replaying queued writes.
use crate::machine::error::SessionError as Error;
mod astm;
pub(crate) mod autolyser;
pub(crate) mod link;
pub(crate) use link::capture;
use link::{folder, serial};
mod hl7;
pub(crate) mod interpretation;
pub use interpretation::reprocess;
mod law;
mod poct;
pub(crate) mod receive;
mod settings;
pub(crate) use receive::{connection_input, custody, error_location, transfer_end};
pub(crate) mod vidas;
mod worker;
use crate::installation::config::Machine;
use analyte_config::TcpRole;
use analyte_store::Handle;
use analyte_transport::{Backoff, Limits, connection};
use settings::{Link, Session};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// Separates completed receive commits from transport closure during shutdown.
pub(crate) struct Closing {
    pub(crate) ready: watch::Sender<bool>,
    pub(crate) close: watch::Receiver<bool>,
}

impl Closing {
    pub(crate) async fn after_commits(&mut self) -> Result<(), Error> {
        self.ready.send_replace(true);
        tokio::time::timeout(Duration::from_secs(25), stopping(&mut self.close))
            .await
            .map_err(Error::TransportCloseStageExceeded25Seconds)?;
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
        return law::independent::run(machine, store, stop, closing, capture).await;
    }
    let link = settings::transport(machine.snapshot.effective().stack());
    let limits = Limits::new(16, Duration::from_millis(u64::from(link.timeout_ms())))?;
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
                () = tokio::time::sleep(Duration::from_millis(100)) => {}
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
        Link::Folder { bytes, .. } => {
            folder::session(
                &machine,
                &store,
                bytes,
                file_root,
                stop,
                closing,
                capture.as_deref(),
            )
            .await
        }
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
    Ok(Backoff::new(
        Duration::from_millis(100),
        Duration::from_secs(5),
        1,
    )?)
}

async fn session(
    machine: &Machine,
    selected: Session<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let context = crate::intake::Connection::new(machine)?;
    let machine = &context;
    match selected {
        Session::Astm(settings) => astm::session(machine, settings, store, conn, stop).await,
        Session::Hl7(settings) => hl7::session(machine, settings, store, conn, stop).await,
        Session::Law(settings) => law::session(machine, settings, store, conn, stop).await,
        Session::Poct(settings) => poct::session(machine, settings, store, conn, stop).await,
        Session::Vidas(settings) => vidas::session(machine, settings, store, conn, stop).await,
        Session::Autolyser(settings) => {
            autolyser::session(machine, settings, store, conn, stop).await
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
    selected: Session<'_>,
    store: &Handle,
    mut conn: analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    closing: &mut Closing,
) -> Result<(), Error> {
    let result = session(machine, selected, store, &mut conn, stop.clone()).await;
    #[cfg(feature = "test-hooks")]
    if let Some(hook) = &machine.connection_closing {
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
