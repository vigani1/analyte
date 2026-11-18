//! Owns send-ahead reservations, queued requests and the candidate cursor until durable completion.
use crate::{installation::config::Machine, orders::OrderError};
use analyte_store::{Handle, MachineOrder};
use std::{collections::VecDeque, time::Instant};

/// Cancellation retains the active reservation for close or restart reconciliation.
pub(crate) struct Work<T> {
    pub(in crate::orders) active: Option<MachineOrder>,
    pub(in crate::orders) requests: VecDeque<T>,
    cursor: String,
    pub(in crate::orders) next_poll: Instant,
}
impl<T> Default for Work<T> {
    fn default() -> Self {
        Self {
            active: None,
            requests: VecDeque::new(),
            cursor: String::new(),
            next_poll: Instant::now(),
        }
    }
}
impl<T> Work<T> {
    /// Advances the bounded idle poll cadence without changing reservations.
    pub(crate) fn poll_due(&mut self) -> Result<bool, OrderError> {
        let now = Instant::now();
        if now < self.next_poll {
            return Ok(false);
        }
        self.next_poll = now
            .checked_add(super::ORDER_POLL_INTERVAL)
            .ok_or(OrderError::OrderPollDeadlineOverflow)?;
        Ok(true)
    }
    /// Reserves one complete command, retaining its identity before host I/O.
    pub(crate) async fn poll_one<E>(
        &mut self,
        store: &Handle,
        machine: &Machine,
        map: impl Fn(&analyte_mapping::AppOrder) -> Result<T, E>,
    ) -> Result<Option<T>, OrderError>
    where
        OrderError: From<E>,
    {
        if !self.poll_due()? {
            return Ok(None);
        }
        if !self
            .reserve(store, machine, |order| {
                map(order).map(|payload| VecDeque::from([payload]))
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
    pub(in crate::orders) async fn reserve<E>(
        &mut self,
        store: &Handle,
        machine: &Machine,
        map: impl Fn(&analyte_mapping::AppOrder) -> Result<VecDeque<T>, E>,
    ) -> Result<bool, OrderError>
    where
        OrderError: From<E>,
    {
        let Some(reserved) = super::next(store, machine, &mut self.cursor, map).await? else {
            return Ok(false);
        };
        self.active = Some(reserved.order);
        self.requests = reserved.payload;
        Ok(true)
    }
    pub(crate) async fn close(&self, store: &Handle, reason: &str) -> Result<(), OrderError> {
        if let Some(order) = &self.active {
            super::uncertain(store, order, reason).await?;
        }
        Ok(())
    }
    pub(crate) async fn refuse(&mut self, store: &Handle, reason: &str) -> Result<(), OrderError> {
        self.close(store, reason).await?;
        self.active = None;
        self.requests.clear();
        Ok(())
    }
}
