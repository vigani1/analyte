//! Supplies LAW dialog effects and transaction deadlines to the shared driver.
use super::{Code, Context, Error, Event, Host};
use crate::machine::{
    connection_input::Input,
    driver::{Effects, Flow},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) enum Mode<'a> {
    Inbound { opened: bool, next_idle: Instant },
    Outbound(&'a analyte_hl7::Message),
}
impl Mode<'_> {
    pub(super) fn inbound() -> Self {
        Self::Inbound {
            opened: false,
            next_idle: Instant::now(),
        }
    }
}
impl Effects for Host<'_, '_> {
    type Settings = analyte_config::LawStack;
    type Error = Error;
    fn wait(&self) -> Result<Option<Duration>, Error> {
        let until = match self.mode {
            Mode::Outbound(_) => self.orders.due().ok_or(Error::OutboundDeadlineAbsent)?,
            Mode::Inbound { next_idle, .. } => {
                let dialog = self
                    .dialog
                    .next_deadline()
                    .map(|until| self.absolute(until))
                    .transpose()?;
                [Some(next_idle), self.orders.due(), dialog]
                    .into_iter()
                    .flatten()
                    .min()
                    .ok_or(Error::ConnectionDeadlineOverflow)?
            }
        };
        Ok(Some(until.saturating_duration_since(Instant::now())))
    }
    fn receive_error(&self, error: analyte_transport::Error) -> Result<Input, Error> {
        match (self.mode, error) {
            (Mode::Inbound { .. }, analyte_transport::Error::Deadline(_)) => Ok(Input::Tick),
            (Mode::Inbound { .. } | Mode::Outbound(_), error) => Err(error.into()),
        }
    }
    async fn input(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        input: Input,
    ) -> Result<Flow, Error> {
        match input {
            Input::Tick => self.tick(host).await?,
            Input::Opened { .. } => match &mut self.mode {
                Mode::Inbound { opened, .. } => *opened = true,
                Mode::Outbound(message) => {
                    let message = *message;
                    let until = self
                        .orders
                        .due()
                        .ok_or(Error::OutboundDeadlineAbsent)?
                        .saturating_duration_since(self.clock);
                    self.drive(Event::Send(message, until), host).await?;
                }
            },
            Input::Bytes { bytes } => self.feed(bytes, host).await?,
            Input::Closed(reason) => match (self.mode, reason) {
                (Mode::Inbound { .. }, _) => return Ok(Flow::Stop),
                (Mode::Outbound(_), None) => return Err(Error::PeerClosedBeforeOrl),
                (Mode::Outbound(_), Some(_)) => return Err(Error::IndependentPeerDisconnected),
            },
            Input::Failed(reason) => match self.mode {
                Mode::Inbound { .. } => {
                    tracing::warn!(machine=self.machine.machine().id.as_str(),error_kind=?reason,"LAW transport attempt failed");
                }
                Mode::Outbound(_) => return Err(Error::TransportFailed(reason)),
            },
            Input::RecordingFailed(_failure) => {}
        }
        Ok(
            if self.stopped || matches!(self.mode, Mode::Outbound(_)) && self.completed {
                Flow::Stop
            } else {
                Flow::Continue
            },
        )
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_, Self::Settings>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let finished = self.finish().await;
        let cleanup = match self.mode {
            Mode::Inbound { .. } => self
                .orders
                .close(
                    self.store,
                    if result.is_err() {
                        Code::LawSessionError.as_str()
                    } else {
                        Code::LawDisconnect.as_str()
                    },
                )
                .await
                .map_err(Into::into),
            Mode::Outbound(_) => Ok(()),
        };
        crate::machine::custody::retain_cleanup(result, finished);
        cleanup
    }
}
impl Host<'_, '_> {
    async fn tick(
        &mut self,
        host: &mut Context<'_, '_, analyte_config::LawStack>,
    ) -> Result<(), Error> {
        let now = Instant::now();
        match self.mode {
            Mode::Outbound(_) => {
                let until = self.orders.due().ok_or(Error::OutboundDeadlineAbsent)?;
                tokio::time::timeout_at(until.into(), std::future::pending::<()>())
                    .await
                    .map_err(Error::IndependentTransactionDeadline)?;
            }
            Mode::Inbound { opened, next_idle } => {
                if self.orders.due().is_some_and(|due| now >= due) {
                    self.orders.tick(self.store).await?;
                } else if self
                    .dialog
                    .next_deadline()
                    .is_some_and(|due| self.clock.elapsed() >= due)
                {
                    self.drive(Event::Tick, host).await?;
                } else if now >= next_idle {
                    let next_idle = now
                        .checked_add(Duration::from_millis(100))
                        .ok_or(Error::ConnectionDeadlineOverflow)?;
                    self.mode = Mode::Inbound { opened, next_idle };
                    if self.outbound.is_none() && opened && !self.dialog.receiving() {
                        let mut stop = host.stop.clone();
                        let outgoing = tokio::select! {
                            biased;
                            () = crate::machine::stopping(&mut stop) => { self.stopped = true; return Ok(()); },
                            result = self.orders.idle(self.store, self.machine.machine(), self.settings.options()) => match result {
                                Ok(message) => message,
                                Err(error) => {
                                    self.orders.close(self.store, Code::LawIdleAdmissionFailed.as_str()).await?;
                                    tracing::error!(machine=self.machine.machine().id.as_str(),error_kind=%error,"LAW idle admission stopped; no command retried");
                                    None
                                }
                            },
                        };
                        if let Some(message) = outgoing {
                            let until = self
                                .orders
                                .due()
                                .ok_or(Error::OutgoingDeadlineAbsent)?
                                .saturating_duration_since(self.clock);
                            self.drive(Event::Send(&message, until), host).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
