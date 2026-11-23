//! Keeps a VIDAS PC talking on a record-only connection with only the Literal upload rules (ADR 0034).
//!
//! The runner journals every chunk before this host sees a byte, so a packet is
//! durably kept before its acknowledgment (ADR 0010), and bytes that fail
//! framing or the checksum are kept too. Uploads are never read and nothing is
//! ever downloaded: VIDAS Literal has no host query to answer.
use super::Error;
use crate::machine::driver::{Flow, HostInput, Protocol, Session};
use crate::machine::receive::custody::{self, ms};
use analyte_config::recording::VidasCapture;
use analyte_dialog::vidas::{Action, Config as DialogConfig, Dialog, Event};
use analyte_framing::vidas::Config;
use std::time::Duration;

/// One record-only VIDAS connection.
pub(in crate::machine) struct Host {
    dialog: Dialog,
    /// Opening and tick actions run in the next step.
    pending: Vec<Action>,
    acknowledgment: Duration,
}

/// The packet envelope of the capture profile and the lab's acknowledgment point.
fn framing(link: &VidasCapture) -> Result<Config, Error> {
    use analyte_framing::vidas::{Checksum, HexOptions, Mode};
    Ok(Config::new(
        match link.ack_point() {
            analyte_config::AckPoint::Etx => Mode::Alternate,
            analyte_config::AckPoint::Checksum => Mode::Literal,
        },
        link.chunk(),
        link.frame_limit(),
        link.suffix(),
        Checksum {
            include_stx: link.include_stx().into(),
            include_suffix: link.include_suffix().into(),
            hex: HexOptions {
                uppercase: link.hex_case().into(),
                accept_either_case: link.checksum_acceptance().into(),
            },
        },
    )?)
}

/// The upload deadlines of the capture profile, including waits for retransmission.
fn dialog(link: &VidasCapture) -> DialogConfig {
    DialogConfig {
        ack: ms(link.acknowledgment_ms()),
        inter_byte: ms(link.interbyte_ms()),
        session: ms(link.session_ms()),
        commit: ms(link.commit_ms()),
        retries: 0,
        backoff: ms(link.acknowledgment_ms()),
    }
}

impl Host {
    pub(in crate::machine) fn open(link: &VidasCapture) -> Result<Self, Error> {
        let mut dialog = Dialog::upload(framing(link)?, dialog(link))?;
        let pending = dialog.start();
        Ok(Self {
            dialog,
            pending,
            acknowledgment: ms(link.acknowledgment_ms()),
        })
    }
}

impl Protocol for Host {
    type Settings = analyte_config::RecordStack;
    type Action = Action;
    type Error = Error;

    fn deadline(&self, _s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        Ok(Some(
            self.dialog
                .next_deadline()
                .ok_or(Error::ActiveDialogHasNoDeadline)?,
        ))
    }

    async fn idle(&mut self, s: &mut Session<'_, '_, Self>) -> Result<Flow, Error> {
        s.actions.extend(self.pending.drain(..));
        Ok(Flow::Continue)
    }

    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        match input {
            HostInput::Opened | HostInput::Chunk => {}
            HostInput::Tick => {
                let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
                self.pending.extend(actions);
            }
            HostInput::Disconnected => {
                let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
                s.actions.extend(actions);
            }
            // A framing failure ends the session; the bytes are already kept.
            HostInput::Byte(byte) => match self.dialog.step(s.elapsed(), Event::Byte(byte)) {
                Ok(actions) => s.actions.extend(actions),
                Err(error) => {
                    s.machine.machine().status.record_failed();
                    return Err(error.into());
                }
            },
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
                let deadline = custody::WriteDeadline::Within(self.acknowledgment);
                custody::write(s.conn, &bytes, deadline).await?;
            }
            // The runner already committed the chunks holding this packet.
            Action::Store(_) | Action::RejectChecksum(_) => {
                if matches!(action, Action::RejectChecksum(_)) {
                    s.machine.machine().status.record_failed();
                } else {
                    s.machine.machine().status.recorded();
                }
                let actions = self.dialog.step(s.elapsed(), Event::Stored)?;
                s.actions.extend(actions);
            }
            Action::TimerArmed(_, _) | Action::TimerCancelled(_) => {}
            Action::Park => {
                s.machine.machine().status.record_failed();
                s.failures.keep(Error::TransferUnreconciled {
                    expired: self.dialog.expired_timer(),
                });
            }
            // One upload per session, as the clinical host: the link reopens for the next.
            Action::Close => s.stop_when_drained(),
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
