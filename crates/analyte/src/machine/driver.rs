//! Owns connection receive waits, input custody and ordered protocol cleanup.
use super::{
    connection_input::{self, Input},
    custody::{self, CleanupFailure},
};
use crate::intake;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Transport input custody follows the protocol's existing durable boundary.
#[derive(Clone, Copy)]
pub(super) enum Custody {
    /// ASTM frames and HL7/LAW messages commit through dialog effects.
    Dialog,
    /// Vendor and POCT input chunks additionally commit before dialog input.
    Chunks,
}
pub(super) enum Flow {
    Continue,
    Stop,
}

/// Shared host ownership; cancellation never cancels a queued durable-store request.
pub(super) struct Context<'a, 'm, S> {
    pub machine: &'a intake::Connection<'m>,
    pub settings: &'a S,
    pub store: &'a Handle,
    pub conn: &'a mut analyte_transport::Connection,
    pub stop: watch::Receiver<bool>,
}

/// A protocol supplies dialog transitions and effects, never its own receive loop.
pub(super) trait Effects {
    type Settings;
    type Error: CleanupFailure + From<super::link::error::Error> + From<analyte_transport::Error>;
    fn custody(&self) -> Custody {
        Custody::Dialog
    }
    fn wait(&self) -> Result<Option<Duration>, Self::Error>;
    async fn prepare(
        &mut self,
        _host: &mut Context<'_, '_, Self::Settings>,
    ) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }
    /// Protocol-specific pending host work races input; dropping it never proves rollback.
    async fn background(
        &mut self,
        _machine: &intake::Connection<'_>,
        _store: &Handle,
    ) -> Result<(), Self::Error> {
        std::future::pending().await
    }
    async fn input(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        input: Input,
    ) -> Result<Flow, Self::Error>;
    /// Retains every owned effect even when another cleanup operation already failed.
    async fn close(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        result: &mut Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}

/// Opens a protocol owner, drives ordered effects, and always attempts its cleanup.
/// A cancelled store or write can still have happened; protocol effects retain its identity.
pub(super) async fn run<P: Effects>(
    open: Result<P, P::Error>,
    mut host: Context<'_, '_, P::Settings>,
) -> Result<(), P::Error> {
    let mut protocol = open?;
    let mut result = drive(&mut protocol, &mut host).await;
    let cleanup = protocol.close(&mut host, &mut result).await;
    custody::retain_cleanup(&mut result, cleanup);
    result
}
async fn drive<P: Effects>(
    protocol: &mut P,
    host: &mut Context<'_, '_, P::Settings>,
) -> Result<(), P::Error> {
    loop {
        if matches!(protocol.prepare(host).await?, Flow::Stop) {
            return Ok(());
        }
        let wait = protocol.wait()?;
        let custody = protocol.custody();
        let input = tokio::select! {
            biased;
            () = super::stopping(&mut host.stop) => {
                match custody {
                    Custody::Dialog => return Ok(()),
                    Custody::Chunks => Input::Closed(None),
                }
            },
            () = wait_for(wait) => Input::Tick,
            input = connection_input::receive(host.conn) => match input {
                Ok(input) => input,
                Err(analyte_transport::Error::Deadline(_)) => Input::Tick,
                Err(error) => return Err(error.into()),
            },
            result = protocol.background(host.machine, host.store) => {
                result?;
                Input::Tick
            },
        };
        if let (Custody::Chunks, Input::Bytes { bytes }) = (custody, &input) {
            intake::receive(
                host.store,
                host.machine,
                analyte_model::ReceiptId::new(),
                bytes.clone(),
            )
            .await
            .map_err(super::link::error::Error::from)?;
        }
        if matches!(protocol.input(host, input).await?, Flow::Stop) {
            return Ok(());
        }
    }
}
async fn wait_for(wait: Option<Duration>) {
    match wait {
        Some(wait) => tokio::time::sleep(wait).await,
        None => std::future::pending().await,
    }
}
