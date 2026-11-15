//! One owner per machine; transport sessions never replay queued writes.
mod astm;
mod autolyser;
mod autolyser_custody;
mod folder;
mod hl7;
mod law;
mod poct;
mod serial;
mod vendor;
mod vendor_model;
mod vidas;
use crate::{Error, config::Machine, invalid};
use analyte_config::TransportKind;
use analyte_store::Handle;
use analyte_transport::{Backoff, Limits, connection};
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
            .map_err(|_| invalid("transport close stage exceeded 25 seconds"))?;
        Ok(())
    }
}

pub(crate) async fn run(
    mut machine: Machine,
    store: Arc<Handle>,
    mut stop: watch::Receiver<bool>,
    file_root: std::path::PathBuf,
    mut closing: Closing,
) -> Result<(), Error> {
    let limits = Limits::new(
        16,
        Duration::from_millis(u64::from(
            machine
                .snapshot
                .effective()
                .machine()
                .transport
                .operation_timeout_ms,
        )),
    )?;
    let endpoint = &machine.snapshot.effective().site().endpoint;
    match machine.snapshot.effective().machine().transport.kind {
        TransportKind::TcpServer => {
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
                let (conn, side) = connection(limits);
                let cancel = conn.cancellation();
                let processing = drive(&machine, &store, conn, stop.clone(), &mut closing);
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
        TransportKind::TcpClient => loop {
            let (conn, side) = connection(limits);
            let transport = analyte_transport::tcp::client(endpoint.parse()?, side, retry()?);
            let (result, transport) = tokio::join!(
                drive(&machine, &store, conn, stop.clone(), &mut closing),
                transport
            );
            result?;
            transport?;
            if *stop.borrow() {
                return Ok(());
            }
            tokio::select! {
                () = stopping(&mut stop) => return Ok(()),
                () = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        },
        TransportKind::Serial => serial::run(&machine, &store, limits, stop, closing).await,
        TransportKind::WatchedFolder => {
            folder::session(&machine, &store, file_root, stop, closing).await
        }
    }
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
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    match machine.snapshot.effective().machine().dialog.kind {
        analyte_config::DialogKind::AstmLink => astm::session(machine, store, conn, stop).await,
        analyte_config::DialogKind::Hl7Receiver => hl7::session(machine, store, conn, stop).await,
        analyte_config::DialogKind::LawManager => law::session(machine, store, conn, stop).await,
        analyte_config::DialogKind::Poct => poct::session(machine, store, conn, stop).await,
        analyte_config::DialogKind::VidasLiteral => {
            vidas::session(machine, store, conn, stop).await
        }
        analyte_config::DialogKind::Autolyser => {
            autolyser::session(machine, store, conn, stop).await
        }
        analyte_config::DialogKind::FilePickup => Err(invalid("file pickup dialog is not wired")),
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
    store: &Handle,
    mut conn: analyte_transport::Connection,
    stop: watch::Receiver<bool>,
    closing: &mut Closing,
) -> Result<(), Error> {
    let result = session(machine, store, &mut conn, stop.clone()).await;
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    conn.cancellation().cancel();
    // Drop closes the event consumer so a transport ending after an error cannot
    // wait behind unread events during shutdown.
    result
}
