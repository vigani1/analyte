//! Cancels eligible durable machine orders.
use super::*;

impl Store {
    /// Rechecks an app cancellation and records its version once, before cancel bytes.
    /// # Errors
    /// Refuses stale/paused feeds, repeated cancellation versions and illegal states.
    pub fn request_order_cancel(
        &mut self,
        id: i64,
        version: NonZeroU64,
        now: i64,
        owner: &Identifier,
        freshness: u64,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        let code = SourceText::new(&format!("app-cancel:{version}:{}", owner.as_str()))
            .map_err(|_| Error::InvalidInput)?;
        self.change_order(
            id,
            OrderState::CancelRequested,
            code,
            now,
            ChangeEvidence::AppCancel(version, freshness, owner),
            build,
        )
    }
}
