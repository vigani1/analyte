//! Keeps an HL7 analyzer talking on a record-only connection with only MLLP blocks and HL7 acknowledgments (ADR 0034).
//!
//! The runner journals every chunk before this host sees a byte, so a message
//! is durably kept before any acknowledgment (ADR 0010), and a block that
//! fails framing or parsing is kept too. Only MSH is read, to answer: a result
//! message (ORU, OUL) gets AA, which says it is safely kept; any other message,
//! a query above all, gets AE, the standard's application error (HL7 2.3.1
//! 2.22 and 2.5.1 2.9.2: a query the host cannot process is answered AE,
//! never AA without data, which would mean "nothing found"). Nothing else is
//! ever sent.
use super::Error;
use super::orders::response::{Profile, response_metadata};
use crate::machine::driver::{Flow, HostInput, Protocol, Session};
use crate::machine::receive::custody::{self, WriteDeadline, ms};
use analyte_config::recording::Hl7Capture;
use analyte_dialog::hl7::{AckChoice, Action, Config, Dialog, Event};
use std::time::Duration;

/// One record-only HL7 connection.
pub(in crate::machine) struct Host {
    dialog: Dialog,
    options: analyte_hl7::Options,
    wake: Option<Duration>,
}

/// The parsing options an acknowledgment needs, from the capture profile.
fn options(link: &Hl7Capture) -> analyte_hl7::Options {
    super::settings::options_with_limits(
        link.encoding(),
        link.terminator(),
        link.escapes(),
        analyte_hl7::Limits {
            message_bytes: link.message_bytes(),
            segments: link.segments(),
            fields: link.fields(),
            repetitions: link.repetitions(),
        },
    )
}

/// Whether MSH-9 names a message that carries results: ORU (HL7 2.5.1 7.3)
/// or OUL (7.3 laboratory automation results).
fn carries_results(payload: &[u8], options: analyte_hl7::Options) -> bool {
    analyte_hl7::Message::parse(payload, options)
        .ok()
        .and_then(|message| {
            let msh = analyte_hl7::Msh::new(&message).ok()?;
            let code = msh.message_type().next()?;
            Some(matches!(code, b"ORU" | b"OUL"))
        })
        .unwrap_or(false)
}

impl Host {
    pub(in crate::machine) fn open(link: &Hl7Capture) -> Result<Self, Error> {
        let options = options(link);
        // One message at a time: a second block waits until the first is answered.
        let config = Config::new(options, ms(link.wait_ms()), 1)?;
        Ok(Self {
            dialog: Dialog::new(
                config,
                analyte_dialog::Epoch(uuid::Uuid::now_v7().as_u128()),
            ),
            options,
            wake: None,
        })
    }

    fn step(&mut self, s: &mut Session<'_, '_, Self>, event: Event<'_>) -> Result<(), Error> {
        let actions = self.dialog.step(s.elapsed(), event)?;
        s.actions.extend(actions);
        Ok(())
    }
}

impl Protocol for Host {
    type Settings = analyte_config::RecordStack;
    type Action = Action;
    type Error = Error;

    fn deadline(&self, _s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        Ok(self.wake)
    }

    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        match input {
            HostInput::Opened | HostInput::Chunk => {}
            HostInput::Byte(byte) => self.step(s, Event::Byte(byte))?,
            HostInput::Tick => self.step(s, Event::Tick)?,
            HostInput::Disconnected => self.step(s, Event::Disconnected)?,
        }
        Ok(Flow::Continue)
    }

    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        let status = &s.machine.machine().status;
        match action {
            // The runner already committed the chunks holding this block.
            Action::Store(token, _) => self.step(s, Event::Stored(token))?,
            Action::Interpret(token, payload) => {
                status.recorded();
                if carries_results(&payload, self.options) {
                    self.step(s, Event::Interpreted(token))?;
                } else {
                    let error = analyte_hl7::AckError::unlocated(
                        analyte_hl7::APPLICATION_INTERNAL_ERROR,
                        b"record-only",
                    );
                    self.step(
                        s,
                        Event::NotInterpretable(token, error, AckChoice::ContentError),
                    )?;
                }
            }
            Action::ResponseNeeded(token, _) => {
                let metadata = response_metadata(Profile::Hl7);
                self.step(s, Event::Respond(token, metadata.response()?))?;
            }
            Action::Write(write) => {
                custody::write(
                    s.conn,
                    &write.bytes,
                    WriteDeadline::dialog(s.started(), write.until)?,
                )
                .await?;
            }
            Action::Deadline(next) => self.wake = next,
            Action::Rejected(_, _) | Action::Parked(_, _) | Action::Unmatched(_) => {
                status.record_failed();
            }
            Action::QueueFull(_) => {
                status.record_failed();
                return Err(Error::PendingReceiptLimitExceeded);
            }
            Action::TimedOut(_) => {
                status.record_failed();
                s.failures.keep(Error::HostPhaseTimeout);
            }
            Action::Refused(error) => s.failures.keep(error.into()),
            Action::ReceiptRetained(_) => {}
            // Orders are never enabled on this dialog, so none of these can arrive.
            Action::Query { .. }
            | Action::OrderResponse { .. }
            | Action::OrderResponseTimeout
            | Action::DisplayOutcome { .. }
            | Action::QueryWrite { .. }
            | Action::Sent(_, _)
            | Action::CommitAccepted(_) => return Err(Error::UnexpectedOutgoingHl7Outcome),
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
