//! Asks an Autolyser only for its next report on a record-only connection, at the lab's interval (ADR 0034).
//!
//! The runner journals every chunk before this host sees a byte, so each
//! response is durably kept before the next R, the read that lets the analyzer
//! move on (ADR 0010). Analyte sends only R (next report): never L (last
//! report), never A (the archive replay) and never a work list. Each R is
//! guarded as on a normal connection: a response that is incomplete, corrupt
//! or not in the expected layout stops polling and keeps the guard uncertain,
//! so neither this session nor a restart sends another R until an operator
//! reconciles it (docs/protocols/autolyser/README.md). Reports are never read.
use super::{Error, custody};
use crate::machine::driver::{Flow, HostInput, Protocol, Session};
use crate::machine::receive::custody::{WriteDeadline, ms, write};
use analyte_config::recording::AutolyserCapture;
use analyte_dialog::autolyser::{Action, Config, Dialog, Event, Recovery, Request};
use std::time::Duration;

/// One record-only Autolyser connection.
pub(in crate::machine) struct Host {
    dialog: Dialog,
    /// The last response was kept and the next R may follow at the interval.
    ready: bool,
    /// Polling stopped for good in this session after an unusable response.
    stopped: bool,
    guard: Option<analyte_store::PollIntentId>,
    acknowledgment: Duration,
}

/// The poll settings of the capture profile and the lab's interval. Recovery
/// is manual: the dialog never asks for L by itself.
fn config(link: &AutolyserCapture, controls: bool) -> Config {
    Config {
        poll: ms(link.poll_ms()),
        ack: ms(link.acknowledgment_ms()),
        response: ms(link.response_ms()),
        inter_byte: ms(link.interbyte_ms()),
        storage: ms(link.storage_ms()),
        max_bytes: link.frame_limit(),
        controls,
        recovery: Recovery::ManualLast,
    }
}

impl Host {
    /// Refuses to start while an earlier session left an R unreconciled.
    pub(in crate::machine) async fn open(
        machine: &crate::installation::config::Machine,
        link: &AutolyserCapture,
        store: &analyte_store::Handle,
    ) -> Result<Self, Error> {
        custody::ensure_reconciled(store, machine).await?;
        Ok(Self {
            dialog: Dialog::new(config(link, link.controls()))?,
            ready: false,
            stopped: false,
            guard: None,
            acknowledgment: ms(link.acknowledgment_ms()),
        })
    }

    /// Guards the cursor, then asks for the next report: the only request sent.
    async fn next(&mut self, s: &mut Session<'_, '_, Self>) -> Result<(), Error> {
        self.ready = false;
        self.guard = Some(custody::reserve(s.store, s.machine.machine()).await?);
        let actions = self.dialog.step(s.elapsed(), Event::Poll(Request::Next))?;
        s.actions.extend(actions);
        Ok(())
    }

    /// Stops polling: the R guard stays uncertain for an operator.
    async fn stop(&mut self, s: &mut Session<'_, '_, Self>, reason: &str) -> Result<(), Error> {
        self.stopped = true;
        self.ready = false;
        s.machine.machine().status.record_failed();
        custody::uncertain(s.store, self.guard.take(), s.machine.machine(), reason).await
    }
}

impl Protocol for Host {
    type Settings = analyte_config::RecordStack;
    type Action = Action;
    type Error = Error;

    fn deadline(&self, _s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        Ok(self.dialog.next_deadline())
    }

    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        match input {
            HostInput::Opened => self.next(s).await?,
            HostInput::Chunk => {}
            HostInput::Byte(byte) => {
                let actions = self.dialog.step(s.elapsed(), Event::Byte(byte))?;
                s.actions.extend(actions);
            }
            HostInput::Tick => {
                let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
                if actions.is_empty() && self.ready && !self.stopped {
                    self.next(s).await?;
                } else {
                    s.actions.extend(actions);
                }
            }
            HostInput::Disconnected => {
                let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
                s.actions.extend(actions);
            }
        }
        Ok(Flow::Continue)
    }

    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        match action {
            Action::Write(bytes) => {
                write(s.conn, &bytes, WriteDeadline::Within(self.acknowledgment)).await?;
            }
            // The runner already committed the chunks holding this response.
            Action::Store(token, _) => {
                let actions = self.dialog.step(s.elapsed(), Event::Stored(token))?;
                s.actions.extend(actions);
            }
            Action::Report(_) => {
                s.machine.machine().status.recorded();
                custody::complete(s.store, &mut self.guard, s.machine.machine()).await?;
                self.ready = true;
            }
            // NAK: no report waiting; the next R follows at the interval.
            Action::Empty => {
                custody::complete(s.store, &mut self.guard, s.machine.machine()).await?;
                self.ready = true;
            }
            Action::Park(error) => {
                let reason = super::refusal_reason(&error);
                self.stop(s, &reason).await?;
            }
            Action::Uncertain => {
                self.stop(s, "Autolyser poll response outcome uncertain")
                    .await?;
            }
            // No work list is ever inserted on a record-only connection.
            Action::Insertion(_) => return Err(Error::UnexpectedInsertionOutcome),
            Action::Refused(error) => return Err(error.into()),
            Action::Close => s.stop_when_drained(),
            Action::Deadline(_) => {}
        }
        Ok(Flow::Continue)
    }

    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let code = super::outcome::session_end(s.trigger(), result);
        custody::uncertain(
            s.store,
            self.guard.take(),
            s.machine.machine(),
            code.as_str(),
        )
        .await
    }
}
