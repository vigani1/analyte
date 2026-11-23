//! Keeps an ASTM analyzer talking on a record-only connection with only the LIS01-A2 link rules (ADR 0034).
//!
//! The runner journals every chunk before this host sees a byte, so each frame
//! is durably kept before its ACK (ADR 0010) and a frame that fails its
//! checksum is kept too. Records are never read: a complete transfer is counted,
//! and a transfer holding a query (a Q record) is answered with the standard's
//! own "error in last request for information" termination (LIS02-A2 12.3,
//! code Q), never with "no information" (rule 9). Nothing else is ever sent.
//!
//! Frames carry one record each (LIS01-A2 6.3.1.1), unless the model declares
//! packed records for the analyzer mode the lab chose (ADR 0015): then a frame
//! whose checksum, number and length pass may hold several. Outbound replies
//! stay one record per frame either way.
use super::Error;
use crate::machine::driver::{Flow, HostInput, Protocol, Session};
use crate::machine::receive::custody::{self, WriteDeadline, ms};
use analyte_config::recording::AstmCapture;
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event, Interrupt, Role, Timers};
use std::time::{Duration, Instant};

/// One record-only ASTM connection.
pub(in crate::machine) struct Host {
    dialog: Dialog,
    /// The transfer now arriving holds a Q record.
    query: bool,
    /// The query reply waiting for a neutral link, and when it goes stale.
    reply: Option<(Vec<Vec<u8>>, Instant)>,
    /// Bytes the store must have room for before an ENQ is accepted.
    admission: usize,
    /// How long a query reply may wait for the link (the ENQ reply budget).
    reply_wait: Duration,
}

/// The LIS01-A2 dialog settings of the capture profile; Analyte is the computer.
fn config(link: &AstmCapture, packed: bool, commit: Duration) -> Result<Config, Error> {
    let timers = Timers {
        establish: ms(link.establish_ms()),
        contention: ms(link.contention_ms()),
        frame: ms(link.frame_ms()),
        receive: ms(link.receive_ms()),
        commit: commit.min(ms(link.commit_ms())),
    };
    Ok(Config::new(
        analyte_framing::astm::Limit::new(link.frame_limit())?,
        timers,
        // LIS01-A2 6.2.7.1: the computer yields to the instrument on contention.
        Role::Computer,
        link.message_bytes(),
        link.frames(),
        Interrupt::Honor,
    )?
    .with_record_boundaries(if packed {
        analyte_framing::astm::RecordBoundaries::Packed
    } else {
        analyte_framing::astm::RecordBoundaries::Separate
    }))
}

impl Host {
    pub(in crate::machine) fn open(
        machine: &crate::installation::config::Machine,
        link: &AstmCapture,
        packed: bool,
    ) -> Result<Self, Error> {
        let config = config(link, packed, custody::budget(machine))?;
        let admission = link
            .frame_limit()
            .checked_add(machine.snapshot.canonical().len())
            .ok_or(Error::AdmissionSizeOverflow)?;
        Ok(Self {
            dialog: Dialog::new(config, Epoch(uuid::Uuid::now_v7().as_u128())),
            query: false,
            reply: None,
            admission,
            reply_wait: ms(link.establish_ms()),
        })
    }

    /// Steps one byte; an ENQ first asks the store for room, so a nearly full
    /// disk refuses the transfer with NAK instead of accepting it (rule 1).
    async fn byte(&mut self, s: &mut Session<'_, '_, Self>, byte: u8) -> Result<(), Error> {
        if byte == analyte_framing::astm::ENQ {
            let expected = self.admission;
            let room = s
                .store
                .request(crate::deadline()?, move |store| store.admission(expected))
                .await;
            if let Err(error) = &room {
                tracing::error!(machine = s.machine.machine().id.as_str(), error_kind = %error, "storage admission refused a recording");
            }
            let actions = self
                .dialog
                .step(s.elapsed(), Event::Capacity(room.is_ok()))?;
            s.actions.extend(actions);
        }
        let actions = self.dialog.step(s.elapsed(), Event::Byte(byte))?;
        s.actions.extend(actions);
        Ok(())
    }
}

impl Protocol for Host {
    type Settings = analyte_config::RecordStack;
    type Action = Action;
    type Error = Error;

    fn deadline(&self, s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        let reply = self.reply.as_ref().map(|(_, until)| {
            s.elapsed()
                .saturating_add(until.saturating_duration_since(Instant::now()))
        });
        Ok(match (self.dialog.next_deadline(), reply) {
            (Some(dialog), Some(reply)) => Some(dialog.min(reply)),
            (dialog, reply) => dialog.or(reply),
        })
    }

    /// A waiting query reply goes out once the link is neutral, and only
    /// while it is fresh; a stale one is dropped, never sent late.
    async fn idle(&mut self, s: &mut Session<'_, '_, Self>) -> Result<Flow, Error> {
        if self.dialog.next_deadline().is_none()
            && let Some((records, until)) = self.reply.take()
            && Instant::now() < until
        {
            let actions = self.dialog.step(s.elapsed(), Event::Send(records))?;
            s.actions.extend(actions);
        }
        if self
            .reply
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            self.reply = None;
        }
        Ok(Flow::Continue)
    }

    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        let event = match input {
            HostInput::Opened | HostInput::Chunk => return Ok(Flow::Continue),
            HostInput::Byte(byte) => {
                self.byte(s, byte).await?;
                return Ok(Flow::Continue);
            }
            HostInput::Tick => Event::Tick,
            HostInput::Disconnected => Event::Disconnected,
        };
        let actions = self.dialog.step(s.elapsed(), event)?;
        s.actions.extend(actions);
        Ok(Flow::Continue)
    }

    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        let status = &s.machine.machine().status;
        match action {
            Action::Write(bytes) => {
                custody::write(s.conn, &bytes, WriteDeadline::Transport).await?;
            }
            // The runner already committed the chunk holding this frame.
            Action::Store(id, _, _) => {
                let actions = self.dialog.step(s.elapsed(), Event::Stored(id))?;
                s.actions.extend(actions);
            }
            Action::Record(record) => {
                self.query |= analyte_astm::starts_query(&record);
            }
            Action::Ended(End::Normal) => {
                status.recorded();
                if std::mem::take(&mut self.query) {
                    let until = Instant::now()
                        .checked_add(self.reply_wait)
                        .ok_or(Error::QueryDeadlineOverflow)?;
                    self.reply = Some((query_error(), until));
                }
            }
            Action::Ended(End::Abort | End::Timeout | End::Interrupt | End::Incomplete)
            | Action::Conflict(_)
            | Action::Rejected(_, _)
            | Action::UnexpectedFrame(_) => {
                self.query = false;
                status.record_failed();
            }
            // A reply attempt is never repeated, whatever its outcome.
            Action::Sent(_) | Action::Deadline(_) => {}
            Action::StoreFailed(_) => return Err(Error::CommitDeadline),
            Action::LimitReached => {
                status.record_failed();
                return Err(Error::ResourceLimitRefusedTransfer);
            }
            Action::Refused(error) => return Err(error.into()),
        }
        Ok(Flow::Continue)
    }

    async fn close(
        &mut self,
        _s: &mut Session<'_, '_, Self>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

/// The standard's answer to a query the host cannot answer now: a header and
/// a terminator with code Q, "error in last request for information"
/// (LIS02-A2 12.3); it ends the request without claiming there is nothing.
fn query_error() -> Vec<Vec<u8>> {
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y%m%d%H%M%S%z")
        .to_string();
    analyte_astm::encode_order_envelope(
        "",
        &timestamp,
        Vec::new(),
        analyte_astm::OrderTermination::QueryError,
    )
}
