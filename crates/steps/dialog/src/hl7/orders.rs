//! Runs deferred HL7 display query exchanges (2.3.1 §2.18 and 2.5.1 §5.9).
use super::{Action, Dialog, Error, ExchangeId, Message, Msh};
use analyte_hl7::{AckCode, ControlId, Version, query};
use std::{collections::VecDeque, time::Duration};

/// The transport issuance currently requested by a deferred-display reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyPart {
    /// Query acknowledgment, always issued before any display.
    Qck,
    /// Reserved display message; never automatically retried.
    Dsr,
}

/// The reply an outgoing message waits for. Neither reply authorizes a resend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// The acknowledgment that MSH-15 and MSH-16 request (HL7 2.5.1 §2.9.2 and §2.9.3).
    Acknowledgment,
    /// An ORR or ORL order response within this budget, which also bounds issuance.
    /// The response is stored before it is reported (HL7 2.3.1 §4.2.2 and 2.5.1 §4.4.2).
    OrderResponse(Duration),
}

/// Host-prepared reply after cache lookup and durable order reservation.
#[derive(Debug)]
pub struct DisplayReply {
    /// QCK built for the actual cache outcome.
    pub acknowledgment: Message,
    /// Ordered pages; the host reserves each page before its write.
    pub displays: VecDeque<Message>,
    /// Total query deadline, relative to the connection clock.
    pub query_deadline: Duration,
    /// Separate issuance deadline for an unavailable/empty acknowledgment.
    pub failure_deadline: Duration,
    /// Application ACK budget, beginning immediately before DSR issuance.
    pub acknowledgment_wait: Duration,
}

#[derive(Debug)]
enum Issuance {
    Qck { starts_display: bool },
    Dsr,
}

#[derive(Debug)]
struct Pages {
    displays: VecDeque<Message>,
    deadline: Duration,
    acknowledgment_wait: Duration,
}

#[derive(Debug)]
enum Waiting {
    Idle,
    Display { id: Vec<u8>, until: Duration },
    Order { until: Duration },
}
#[derive(Debug, Clone, Copy)]
enum Kind {
    Query,
    Display,
    Order,
}
#[derive(Debug)]
pub(super) struct Orders {
    version: Version,
    profile: query::Profile,
    pages: Option<Pages>,
    waiting: Waiting,
    pending: Option<(ExchangeId, Message, Kind, Duration)>,
    issuance: Option<Issuance>,
}
impl Orders {
    pub(super) fn committing(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn due(&self) -> Option<Duration> {
        match self.waiting {
            Waiting::Idle => None,
            Waiting::Display { until, .. } | Waiting::Order { until } => Some(until),
        }
    }
    pub(super) fn expire(&mut self, now: Duration, out: &mut Vec<Action>) {
        if self.due().is_some_and(|until| now >= until) {
            self.pages = None;
            match std::mem::replace(&mut self.waiting, Waiting::Idle) {
                Waiting::Idle => {}
                Waiting::Display { .. } => out.push(Action::DisplayOutcome {
                    receipt: None,
                    accepted: false,
                    code: "HL7-DSR-ACK-timeout",
                    detail: None,
                }),
                Waiting::Order { .. } => out.push(Action::OrderResponseTimeout),
            }
        }
    }
    pub(super) fn failed_commit(&mut self, id: ExchangeId) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|(expected, ..)| *expected == id)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }
}
impl Dialog {
    /// Enables orders with the explicitly configured legacy query profile.
    ///
    /// # Errors
    /// Rejects reconfiguration of an active or already configured dialog.
    pub fn enable_order_profile(
        &mut self,
        version: Version,
        profile: query::Profile,
    ) -> Result<(), Error> {
        if self.orders.is_some()
            || self.state.pending().is_some()
            || !self.decoder.pending().is_empty()
        {
            return Err(Error::Busy);
        }
        self.orders = Some(Orders {
            version,
            profile,
            pages: None,
            waiting: Waiting::Idle,
            pending: None,
            issuance: None,
        });
        Ok(())
    }
    /// Returns the next ordinary, sending or order/display deadline without advancing time.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        let sending = self.sending.as_ref().and_then(|s| s.until);
        [
            self.deadline,
            sending,
            self.orders.as_ref().and_then(Orders::due),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    /// Whether an order/display wait or a partial input block prevents new work.
    #[must_use]
    pub fn orders_busy(&self) -> bool {
        !self.decoder.pending().is_empty()
            || self
                .orders
                .as_ref()
                .is_some_and(|orders| !matches!(orders.waiting, Waiting::Idle))
    }
    /// Registers the exact DSR identity before issuance. Cancellation never authorizes
    /// a resend. Call `step` to publish the next deadline.
    /// # Errors
    /// Rejects a busy/disabled exchange or an unrepresentable deadline.
    pub fn await_display(
        &mut self,
        now: Duration,
        id: ControlId<'_>,
        wait: Duration,
    ) -> Result<(), Error> {
        let (orders, until) = self.prepare_wait(now, wait)?;
        orders.waiting = Waiting::Display {
            id: id.raw().to_vec(),
            until,
        };
        Ok(())
    }
    /// Registers the order-response wait, then issues the order within that same budget.
    /// A partial inbound block keeps the link busy. A failed or uncertain issuance never
    /// authorizes a resend.
    pub(super) fn send_order(
        &mut self,
        raw: &[u8],
        wait: Duration,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        if !self.decoder.pending().is_empty() {
            return Err(Error::Busy);
        }
        let limit = self.config.limit;
        let (orders, until) = self.prepare_wait(self.clock.now(), wait)?;
        let write = crate::Write::encode(raw, limit, Some(until))?;
        orders.waiting = Waiting::Order { until };
        out.push(Action::Write(write));
        Ok(())
    }
    // Validate both outbound waits in the same order before changing state.
    fn prepare_wait(
        &mut self,
        now: Duration,
        wait: Duration,
    ) -> Result<(&mut Orders, Duration), Error> {
        if wait.is_zero() {
            return Err(Error::Configuration);
        }
        if now < self.clock.now() {
            return Err(Error::TimeReversed);
        }
        let until = now.checked_add(wait).ok_or(Error::Exhausted)?;
        let orders = self.orders.as_mut().ok_or(Error::Configuration)?;
        if !matches!(orders.waiting, Waiting::Idle) {
            return Err(Error::Busy);
        }
        Ok((orders, until))
    }
    pub(super) fn prepare_reply(
        &mut self,
        reply: &DisplayReply,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let orders = self.orders.as_mut().ok_or(Error::Configuration)?;
        if orders.issuance.is_some() {
            return Err(Error::Busy);
        }
        let starts_display = !reply.displays.is_empty();
        if starts_display && (orders.pages.is_some() || !matches!(orders.waiting, Waiting::Idle)) {
            return Err(Error::Busy);
        }
        let until = if starts_display {
            reply.query_deadline
        } else {
            reply.failure_deadline
        };
        let bytes = analyte_framing::hl7::encode(reply.acknowledgment.raw(), self.config.limit)?;
        if starts_display {
            orders.pages = Some(Pages {
                displays: reply.displays.clone(),
                deadline: reply.query_deadline,
                acknowledgment_wait: reply.acknowledgment_wait,
            });
        }
        orders.issuance = Some(Issuance::Qck { starts_display });
        out.push(Action::QueryWrite {
            bytes,
            until,
            part: ReplyPart::Qck,
        });
        Ok(())
    }
    pub(super) fn reply_written(
        &mut self,
        part: ReplyPart,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let orders = self.orders.as_mut().ok_or(Error::Configuration)?;
        match (orders.issuance.take(), part) {
            (Some(Issuance::Qck { starts_display }), ReplyPart::Qck) => {
                if starts_display {
                    self.next_display(out)?;
                }
            }
            (Some(Issuance::Dsr), ReplyPart::Dsr) => {}
            (state, _) => {
                orders.issuance = state;
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
    fn next_display(&mut self, out: &mut Vec<Action>) -> Result<(), Error> {
        let orders = self.orders.as_mut().ok_or(Error::Configuration)?;
        let Some(pages) = &mut orders.pages else {
            return Ok(());
        };
        let Some(display) = pages.displays.pop_front() else {
            orders.pages = None;
            return Ok(());
        };
        let until = pages.deadline;
        let wait = pages.acknowledgment_wait;
        if self.clock.now() >= until {
            orders.pages = None;
            out.push(Action::DisplayOutcome {
                receipt: None,
                accepted: false,
                code: "HL7-query-deadline-before-DSR",
                detail: None,
            });
            return Ok(());
        }
        let id = ControlId::new(Msh::new(&display)?.control_id())?;
        let bytes = analyte_framing::hl7::encode(display.raw(), self.config.limit)?;
        self.await_display(self.clock.now(), id, wait)?;
        self.orders.as_mut().ok_or(Error::Configuration)?.issuance = Some(Issuance::Dsr);
        out.push(Action::QueryWrite {
            bytes,
            until,
            part: ReplyPart::Dsr,
        });
        Ok(())
    }

    pub(super) fn reply_failed(
        &mut self,
        part: ReplyPart,
        out: &mut Vec<Action>,
    ) -> Result<(), Error> {
        let orders = self.orders.as_mut().ok_or(Error::Configuration)?;
        let code = match (&orders.issuance, part) {
            (Some(Issuance::Qck { starts_display }), ReplyPart::Qck) => {
                if !starts_display {
                    orders.issuance = None;
                    return Ok(());
                }
                "HL7-QCK-write-failed"
            }
            (Some(Issuance::Dsr), ReplyPart::Dsr) => {
                orders.waiting = Waiting::Idle;
                "HL7-DSR-write-failed"
            }
            (None | Some(Issuance::Qck { .. } | Issuance::Dsr), _) => return Err(Error::Stale),
        };
        orders.issuance = None;
        orders.pages = None;
        out.push(Action::DisplayOutcome {
            receipt: None,
            accepted: false,
            code,
            detail: None,
        });
        Ok(())
    }
    pub(super) fn route_order(
        &mut self,
        raw: &[u8],
        message: &Message,
        out: &mut Vec<Action>,
    ) -> bool {
        let Some(orders) = &mut self.orders else {
            return false;
        };
        let Ok(header) = Msh::new(message) else {
            return false;
        };
        let kind = match header.message_type().next() {
            Some(b"QRY") => Kind::Query,
            Some(b"ACK") if matches!(orders.waiting, Waiting::Display { .. }) => Kind::Display,
            Some(b"ORR" | b"ORL") if matches!(orders.waiting, Waiting::Order { .. }) => Kind::Order,
            _ => return false,
        };
        let Some(next) = self.next.checked_add(1) else {
            out.push(Action::Parked(raw.to_vec(), Error::Exhausted));
            return true;
        };
        let id = ExchangeId(self.epoch.0, self.next);
        self.next = next;
        orders.pending = Some((
            id,
            message.clone(),
            kind,
            self.frame_started.unwrap_or(self.clock.now()),
        ));
        self.deadline = None;
        out.push(Action::Store(id, raw.to_vec()));
        true
    }
    pub(super) fn complete_order_commit(
        &mut self,
        id: ExchangeId,
        out: &mut Vec<Action>,
    ) -> Result<bool, Error> {
        let Some(orders) = &mut self.orders else {
            return Ok(false);
        };
        let Some((expected, ..)) = orders.pending.as_ref() else {
            return Ok(false);
        };
        if *expected != id {
            return Err(Error::Stale);
        }
        let (_, message, kind, started) = orders.pending.take().ok_or(Error::Stale)?;
        match kind {
            Kind::Query => out.push(Action::Query {
                receipt: id,
                edition_matches: Msh::new(&message)?.version() == orders.version,
                message,
                started,
            }),
            Kind::Order => {
                orders.waiting = Waiting::Idle;
                out.push(Action::OrderResponse {
                    receipt: id,
                    message,
                });
            }
            Kind::Display => {
                let Waiting::Display { id: control, .. } =
                    std::mem::replace(&mut orders.waiting, Waiting::Idle)
                else {
                    out.push(Action::ReceiptRetained(id));
                    return Ok(true);
                };
                let (accepted, code, detail) = if Msh::new(&message)?.version() != orders.version {
                    (
                        false,
                        "HL7-DSR-ACK-invalid-edition",
                        Some("HL7 DSR ACK edition differs from configured edition".to_owned()),
                    )
                } else {
                    match query::DisplayAck::with_profile(
                        &message,
                        ControlId::new(&control)?,
                        orders.profile,
                    ) {
                        Ok(ack) => match ack.code() {
                            AckCode::Aa => (true, "HL7-DSR-ACK-AA", None),
                            AckCode::Ae => (false, "HL7-DSR-ACK-AE", None),
                            AckCode::Ar => (false, "HL7-DSR-ACK-AR", None),
                            AckCode::Ca | AckCode::Ce | AckCode::Cr => return Err(Error::Stale),
                        },
                        Err(error) => (false, "HL7-DSR-ACK-invalid", Some(error.to_string())),
                    }
                };
                out.push(Action::DisplayOutcome {
                    receipt: Some(id),
                    accepted,
                    code,
                    detail,
                });
                if accepted {
                    self.next_display(out)?;
                } else {
                    orders.pages = None;
                }
            }
        }
        Ok(true)
    }
}
