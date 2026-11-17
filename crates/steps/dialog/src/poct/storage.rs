//! Requests POCT durable custody and drains ordered storage outcomes.
use super::*;

impl Dialog {
    pub(super) fn store(
        &mut self,
        message: Box<Message>,
        purpose: Purpose,
        actions: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let sequence = self.next()?;
        let ticket = CommitId {
            session: self.session.clone(),
            sequence,
        };
        let received = if matches!(purpose, Purpose::Hello) {
            self.hello_received
        } else {
            self.clock.now()
        };
        let deadline = received
            .checked_add(self.ack_timeout)
            .ok_or(Error::Exhausted)?;
        self.pending.push_back(Pending {
            ticket: ticket.clone(),
            reference: message.header().control_id.clone(),
            purpose,
            deadline,
            outcome: None,
        });
        actions.push(Action::Store { ticket, message });
        Ok(())
    }

    pub(super) fn store_completed(
        &mut self,
        ticket: &CommitId,
        outcome: Result<StoreOutcome, ()>,
        actions: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let pending = self
            .pending
            .iter_mut()
            .find(|p| &p.ticket == ticket)
            .ok_or(Error::Commit)?;
        if pending.outcome.is_some() {
            return Err(Error::Commit);
        }
        pending.outcome = Some(outcome);
        if !matches!(self.phase, Phase::Ending(_)) {
            self.drain(actions)?;
        }
        Ok(())
    }

    pub(super) fn drain(&mut self, actions: &mut Vec<Action>) -> Result<(), Error> {
        while self.pending.front().is_some_and(|p| p.outcome.is_some()) {
            let pending = self.pending.pop_front().ok_or(Error::Commit)?;
            match pending.outcome.ok_or(Error::Commit)? {
                Ok(StoreOutcome::Stored) => {
                    self.send(Outbound::Accept(pending.reference), actions)?;
                    match pending.purpose {
                        Purpose::Hello => {
                            if !matches!(self.phase, Phase::RemoteEnd) {
                                self.phase = Phase::Status;
                            }
                        }
                        Purpose::Status => {
                            if !matches!(self.phase, Phase::RemoteEnd) {
                                self.request(actions)?;
                            }
                        }
                        Purpose::Observation => {}
                        Purpose::Termination => {
                            self.close(true, actions);
                            break;
                        }
                    }
                }
                Ok(StoreOutcome::ApplicationError(error)) => {
                    self.send(
                        Outbound::ApplicationError {
                            reference: pending.reference,
                            error,
                        },
                        actions,
                    )?;
                    match pending.purpose {
                        Purpose::Observation => {}
                        Purpose::Hello | Purpose::Status | Purpose::Termination => {
                            self.close(false, actions);
                            break;
                        }
                    }
                }
                Err(()) => {
                    self.send(
                        Outbound::Escape {
                            reference: pending.reference,
                            reason: EscapeReason::CannotComplete,
                        },
                        actions,
                    )?;
                    self.terminate(TerminationReason::Abnormal, actions)?;
                    break;
                }
            }
        }
        self.complete_topic(actions);
        Ok(())
    }
}
