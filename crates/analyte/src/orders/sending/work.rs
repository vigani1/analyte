//! Owns send-ahead reservations, queued requests and the candidate cursor until durable completion.
use crate::{
    installation::config::Machine,
    orders::{OrderError, outcome::Transfer},
};
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
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) fn next_poll(&self) -> Instant {
        self.next_poll
    }
    pub(in crate::orders) async fn reserve(
        &mut self,
        store: &Handle,
        machine: &Machine,
        requests: impl FnOnce(analyte_mapping::MachineOrder) -> Result<VecDeque<T>, OrderError>,
    ) -> Result<bool, OrderError> {
        let Some(reserved) = super::next(store, machine, &mut self.cursor).await? else {
            return Ok(false);
        };
        self.active = Some(reserved.order);
        self.requests = requests(reserved.payload)?;
        Ok(true)
    }
    pub(crate) async fn close(&self, store: &Handle, reason: Transfer) -> Result<(), OrderError> {
        if let Some(order) = &self.active {
            super::uncertain(store, order, reason).await?;
        }
        Ok(())
    }
    pub(crate) async fn refuse(
        &mut self,
        store: &Handle,
        reason: Transfer,
    ) -> Result<(), OrderError> {
        self.close(store, reason).await?;
        self.active = None;
        self.requests.clear();
        Ok(())
    }
}
