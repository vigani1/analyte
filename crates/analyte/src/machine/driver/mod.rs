//! Owns connection receive waits, input custody and ordered protocol cleanup.
use super::{
    connection_input::{self, Input},
    receive::custody::{self, CleanupFailure},
};
use std::{collections::VecDeque, time::Duration};
use tokio::sync::watch;

mod protocol;
pub(super) use protocol::{HostInput, Protocol, Session, Trigger, serve};
/// Exercises each concrete host without opening a real analyzer connection.
#[cfg(test)]
mod tests;
/// The one result shape of every driver step and every host effect.
///
/// Each host runs its dialog actions through one queue loop ([`drain`] for an
/// [`Effects`] host, [`serve`] for a [`Protocol`] host; LAW keeps its own
/// deadline-bounded loop) returning `Result<Flow, Error>`: `Stop` ends the
/// session without an error and an error ends it with that cause. A failure
/// that must wait until the dialog's own queued replies (NAK, refusal, park)
/// have run is kept in the run's one [`Failures`] and reported when the
/// run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Flow {
    /// Keep serving this connection.
    Continue,
    /// End this session normally.
    Stop,
}

/// Holds the failures one run defers until its actions have run.
/// Nothing kept is dropped: a second failure, or an effect error that ends the
/// queue early, joins the first one as its effect.
pub(super) struct Failures<E>(Option<E>);

impl<E: CleanupFailure> Failures<E> {
    pub(super) const fn new() -> Self {
        Self(None)
    }
    /// Keeps a failure; the first one stays the original cause.
    pub(super) fn keep(&mut self, error: E) {
        self.0 = Some(custody::effect_failure(self.0.take(), error));
    }
    /// Hands a kept failure to the action that reports it inside the queue.
    pub(super) fn take(&mut self) -> Option<E> {
        self.0.take()
    }
    /// Ends one run: a kept failure outranks the flow, and an error that
    /// ended the queue early is retained beside it.
    pub(super) fn finish(self, result: Result<Flow, E>) -> Result<Flow, E> {
        match (self.0, result) {
            (Some(kept), Ok(_)) => Err(kept),
            (Some(kept), Err(effect)) => Err(E::with_effect(kept, effect)),
            (None, result) => result,
        }
    }
}

/// One [`drain`] run: the actions still to run, the failures that wait for
/// them, and the connection and stop signal its effects use.
pub(super) struct Run<'a, A, E> {
    /// Actions still to run; an effect appends the dialog's follow-up actions.
    pub actions: VecDeque<A>,
    /// Failures reported once the run ends.
    pub failures: Failures<E>,
    pub conn: &'a mut analyte_transport::Connection,
    pub stop: &'a mut watch::Receiver<bool>,
}

/// A host runs one dialog action at a time; [`drain`] owns the queue loop.
pub(super) trait Drain: Effects {
    type Action;
    /// Runs one action. `Stop` ends the run at once; follow-up actions and
    /// deferred failures join `run`.
    async fn effect(
        &mut self,
        action: Self::Action,
        run: &mut Run<'_, Self::Action, Self::Error>,
    ) -> Result<Flow, Self::Error>;
}

/// Runs a host's actions in order until none is left, an effect stops the run
/// or an effect fails. A kept failure outranks the flow, and an error that
/// ended the run early is retained beside it.
pub(super) async fn drain<H: Drain>(
    host: &mut H,
    actions: impl IntoIterator<Item = H::Action>,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
) -> Result<Flow, H::Error> {
    let mut run = Run {
        actions: actions.into_iter().collect(),
        failures: Failures::new(),
        conn,
        stop,
    };
    let result = async {
        while let Some(action) = run.actions.pop_front() {
            if host.effect(action, &mut run).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }
    .await;
    run.failures.finish(result)
}

/// The connection and stop signal an [`Effects`] host serves; cancellation never
/// cancels a queued durable-store request.
pub(super) struct Context<'a> {
    pub conn: &'a mut analyte_transport::Connection,
    pub stop: watch::Receiver<bool>,
}

/// A protocol supplies dialog transitions and effects, never its own receive loop.
pub(super) trait Effects {
    type Error: CleanupFailure + From<super::link::error::Error> + From<analyte_transport::Error>;
    fn wait(&self) -> Result<Option<Duration>, Self::Error>;
    fn receive_error(&self, error: analyte_transport::Error) -> Result<Input, Self::Error> {
        Ok(tick_on_deadline(error)?)
    }
    /// Protocol-specific pending host work races input; dropping it never proves rollback.
    async fn background(&mut self) -> Result<(), Self::Error> {
        std::future::pending().await
    }
    async fn input(&mut self, host: &mut Context<'_>, input: Input) -> Result<Flow, Self::Error>;
    /// Retains every owned effect even when another cleanup operation already failed.
    async fn close(
        &mut self,
        host: &mut Context<'_>,
        result: &mut Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}

/// Opens a protocol owner, drives ordered effects, and always attempts its cleanup.
/// A cancelled store or write can still have happened; protocol effects retain its identity.
/// Returns only after the session's queued replies have left the transport, so the
/// owner's close cannot discard a final refusal (ADR 0003: refuse visibly).
pub(super) async fn run<P: Effects>(
    open: impl FnOnce() -> Result<P, P::Error>,
    mut host: Context<'_>,
) -> Result<(), P::Error> {
    let mut protocol = open()?;
    let mut result = drive(&mut protocol, &mut host).await;
    let cleanup = protocol.close(&mut host, &mut result).await;
    custody::retain_cleanup(&mut result, cleanup);
    custody::retain_cleanup(&mut result, sent(host.conn).await.map_err(Into::into));
    result
}

/// Waits until every write the session queued has been handed to the operating system.
/// Writes only queue bytes, and the owner cancels the transport once this session
/// returns, so without this wait a refusal written just before an error return (an HL7
/// AR or an ASTM NAK after failed storage) could be dropped unsent. An offline session
/// was ended by the transport, which reports its own reason; nothing more can be sent.
async fn sent(conn: &analyte_transport::Connection) -> Result<(), analyte_transport::Error> {
    match conn.flush().await {
        Err(analyte_transport::Error::Offline) => Ok(()),
        flushed @ (Ok(())
        | Err(
            analyte_transport::Error::InvalidLimits
            | analyte_transport::Error::OversizedWrite
            | analyte_transport::Error::ConsumerClosed
            | analyte_transport::Error::Deadline(_)
            | analyte_transport::Error::Io { .. },
        )) => flushed,
    }
}
async fn drive<P: Effects>(protocol: &mut P, host: &mut Context<'_>) -> Result<(), P::Error> {
    loop {
        let wait = protocol.wait()?;
        let input = tokio::select! {
            biased;
            _ = async { let _ = host.stop.wait_for(|&stopped| stopped).await; } => return Ok(()),
            () = wait_for(wait) => Input::Tick,
            input = connection_input::receive(host.conn) => match input {
                Ok(input) => input,
                Err(error) => protocol.receive_error(error)?,
            },
            result = protocol.background() => {
                result?;
                Input::Tick
            },
        };
        if matches!(protocol.input(host, input).await?, Flow::Stop) {
            return Ok(());
        }
    }
}
/// A receive deadline is a tick; every other transport failure ends the session.
fn tick_on_deadline(error: analyte_transport::Error) -> Result<Input, analyte_transport::Error> {
    match error {
        analyte_transport::Error::Deadline(_) => Ok(Input::Tick),
        error @ (analyte_transport::Error::InvalidLimits
        | analyte_transport::Error::OversizedWrite
        | analyte_transport::Error::ConsumerClosed
        | analyte_transport::Error::Offline
        | analyte_transport::Error::Io { .. }) => Err(error),
    }
}
async fn wait_for(wait: Option<Duration>) {
    match wait {
        Some(wait) if wait.is_zero() => {}
        Some(wait) => tokio::time::sleep(wait).await,
        None => std::future::pending().await,
    }
}
