//! Records individual machine order item outcomes.
use super::{
    Error, ItemDecision, ItemOutcome, MachineOrder, MachineOrderItem, OrderEventDelivery,
    OrderState, OrderTransition, SourceText, SqlOrderState, Store, TransactionBehavior, aggregate,
    append, item_decision, load, params, write_item,
};

impl Store {
    /// Commits completed exchange outcomes, conservative aggregation and one app event.
    /// Missing decisions become Uncertain. Duplicate/unknown items and illegal moves
    /// roll back the entire exchange, including its event. Cancellation cannot resend work.
    /// # Errors
    /// Refuses invalid identities, illegal item transitions, bounds and storage failures.
    pub fn finish_order_items(
        &mut self,
        id: i64,
        outcomes: &[ItemOutcome],
        now: i64,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<MachineOrder, Error> {
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut order = load(&tx, id)?;
        if !matches!(
            order.state,
            OrderState::Sending | OrderState::Sent | OrderState::CancelRequested
        ) || now < order.updated_at
            || outcomes.len() > order.items.len()
            || order.items.is_empty()
        {
            return Err(Error::InvalidOrderInput);
        }
        let mut seen = std::collections::HashSet::new();
        for outcome in outcomes {
            if !seen.insert(outcome.index)
                || !order.items.iter().any(|item| item.index == outcome.index)
            {
                return Err(Error::InvalidOrderInput);
            }
        }
        let cancelling = order.state == OrderState::CancelRequested;
        let mut refused = false;
        for item in &mut order.items {
            if let Some(outcome) = outcomes.iter().find(|outcome| outcome.index == item.index) {
                let next = item_decision(item, outcome.decision)?;
                refused |= matches!(outcome.decision, ItemDecision::CancelRefused);
                if matches!(outcome.decision, ItemDecision::CancelRefused)
                    && item.analyzer_cancelled()
                {
                    tx.execute("INSERT INTO parked(evidence) VALUES (?1)", [crate::parked::evidence("law_cancel_refusal_conflict", &id.to_string(), "LAB-28 UC conflicts with retained analyzer CA; reconcile both facts without another machine command", None)?])?;
                }
                item.state = next;
                item.source_code = outcome.code.clone();
            } else {
                item.state = item_decision(item, ItemDecision::Uncertain)?;
                item.source_code = SourceText::new("exchange-answer-missing")
                    .map_err(crate::StorageError::Text)?;
            }
            item.updated_at = now;
            write_item(&tx, item, limit)?;
        }
        let states = order
            .items
            .iter()
            .map(|item| item.state)
            .collect::<Vec<_>>();
        let mut state = aggregate(&states);
        if cancelling
            && refused
            && !order
                .items
                .iter()
                .any(|item| item.state == OrderState::Uncertain)
        {
            let previous = order
                .items
                .iter()
                .map(|item| {
                    let previous = item.cancel_from.ok_or(Error::Corruption)?;
                    Ok(OrderState::CancelRequested
                        .transition(OrderTransition::CancelRefused(previous))?)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            state = aggregate(&previous);
        }
        // Aggregation is a distinct rule: mixed item outcomes may produce Uncertain
        // even when one item is terminal. It never manufactures a Sent event.
        order.state = state;
        order.updated_at = now;
        order.source_code = outcome_summary(&order.items, refused)?;
        tx.execute(
            "UPDATE machine_orders SET state=?1,updated_at=?2,source_code=?3 WHERE id=?4",
            params![
                SqlOrderState(order.state),
                now,
                order.source_code.as_str(),
                id
            ],
        )?;
        append(&tx, &order, Some(&build(&order)?), limit)?;
        tx.commit()?;
        Ok(order)
    }
}

fn outcome_summary(items: &[MachineOrderItem], refused: bool) -> Result<SourceText, Error> {
    let prefix = if refused {
        "CancelRefused"
    } else {
        "item-outcomes"
    };
    let summary = items
        .iter()
        .map(|item| {
            format!(
                "{}={}:{}",
                item.index,
                item.state,
                item.source_code
                    .as_str()
                    .split(':')
                    .next()
                    .filter(|code| {
                        code.len() <= 64
                            && code.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                            })
                    })
                    .unwrap_or("see-item-history")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    SourceText::new(&format!("{prefix}: {summary}"))
        .map_err(|cause| Error::Storage(crate::StorageError::Text(cause)))
}
