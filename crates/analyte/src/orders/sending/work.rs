//! Owns send-ahead reservations, queued requests and the candidate cursor until durable completion.
use crate::{installation::config::Machine, orders::OrderError};
use analyte_store::{Handle, MachineOrder};
use std::{collections::VecDeque, time::Instant};

/// Cancellation retains the active reservation for close or restart reconciliation.
pub(crate) struct Work<'a, S, T> {
    /// Send-ahead order settings; without them the connection never reserves work.
    orders: Option<&'a S>,
    pub(crate) active: Option<MachineOrder>,
    pub(crate) requests: VecDeque<T>,
    cursor: String,
    reservations: super::Reservations,
    pub(crate) next_poll: Instant,
}
impl<'a, S, T> Work<'a, S, T> {
    /// Takes the settings only when the machine is configured to receive work ahead.
    pub(crate) fn new(orders: Option<&'a S>) -> Result<Self, analyte_model::TextError> {
        Ok(Self {
            orders,
            active: None,
            requests: VecDeque::new(),
            cursor: String::new(),
            reservations: super::Reservations::new()?,
            next_poll: Instant::now(),
        })
    }
    /// Advances the bounded idle poll cadence without changing reservations.
    pub(crate) fn poll_due(&mut self) -> Result<bool, OrderError> {
        let now = Instant::now();
        if now < self.next_poll {
            return Ok(false);
        }
        self.next_poll = now
            .checked_add(super::ORDER_POLL_INTERVAL)
            .ok_or(OrderError::PollDeadlineOverflow)?;
        Ok(true)
    }
    /// Reserves one complete command, retaining its identity before host I/O.
    pub(crate) async fn poll_one<E: From<OrderError> + std::fmt::Display>(
        &mut self,
        store: &Handle,
        machine: &Machine,
        map: impl Fn(&analyte_mapping::AppOrder, &S) -> Result<T, E>,
    ) -> Result<Option<T>, E> {
        if !self.poll_due()? {
            return Ok(None);
        }
        if !self
            .reserve(store, machine, |order, orders| {
                map(order, orders).map(|payload| VecDeque::from([payload]))
            })
            .await?
        {
            return Ok(None);
        }
        Ok(self.requests.pop_front())
    }
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) fn next_poll(&self) -> Instant {
        self.next_poll
    }
    pub(crate) async fn reserve<E: From<OrderError> + std::fmt::Display>(
        &mut self,
        store: &Handle,
        machine: &Machine,
        map: impl Fn(&analyte_mapping::AppOrder, &S) -> Result<VecDeque<T>, E>,
    ) -> Result<bool, E> {
        let Some(orders) = self.orders else {
            return Ok(false);
        };
        let next = super::next(
            store,
            machine,
            &mut self.cursor,
            &mut self.reservations,
            |order| map(order, orders),
        );
        let Some(reserved) = next.await? else {
            return Ok(false);
        };
        self.active = Some(reserved.order);
        self.requests = reserved.payload;
        Ok(true)
    }
    pub(crate) async fn close(&self, store: &Handle, reason: &str) -> Result<(), OrderError> {
        self.reservations.settle(store, reason).await
    }
    pub(crate) async fn refuse(&mut self, store: &Handle, reason: &str) -> Result<(), OrderError> {
        self.close(store, reason).await?;
        self.clear_active();
        self.requests.clear();
        Ok(())
    }
    /// Releases cleanup ownership only after the outcome is durable.
    pub(crate) fn clear_active(&mut self) {
        self.active = None;
        self.reservations.keys.clear();
    }
}
