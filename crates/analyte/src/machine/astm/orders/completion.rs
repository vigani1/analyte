//! Settles ASTM transport completion without implying application acceptance.
use crate::{machine::astm::Error, orders::sending::Work};
use analyte_dialog::astm::outcome::Code as AstmEnd;
use analyte_model::OrderState;
use analyte_store::{Handle, MachineOrder};

/// Settles link completion, keeping a possible application replacement uncertain.
pub(crate) async fn finished(
    store: &Handle,
    order: &MachineOrder,
    end: analyte_dialog::astm::End,
    may_replace: bool,
) -> Result<(), Error> {
    // LIS01 confirms transfer, not the analyzer application's acceptance.
    // Only explicit application evidence may advance Sent to Accepted.
    use analyte_dialog::astm::End;
    let (state, code) = match end {
        End::Normal if may_replace => (OrderState::Uncertain, AstmEnd::ReplacementUnverified),
        End::Normal => (OrderState::Sent, AstmEnd::Confirmed),
        End::Abort => (OrderState::Uncertain, AstmEnd::Aborted),
        End::Timeout => (OrderState::Uncertain, AstmEnd::Timeout),
        End::Interrupt => (OrderState::Uncertain, AstmEnd::Interrupted),
        End::Incomplete => (OrderState::Uncertain, AstmEnd::Incomplete),
    };
    Ok(crate::orders::sending::complete(store, order, state, code.as_str()).await?)
}

impl<S, T> Work<'_, S, T> {
    /// LIS01 confirms transport only; application acceptance remains separate.
    pub(crate) async fn astm_finished(
        &mut self,
        store: &Handle,
        end: analyte_dialog::astm::End,
        may_replace: bool,
    ) -> Result<(), Error> {
        if let Some(order) = &self.active {
            finished(store, order, end, may_replace).await?;
        }
        self.clear_active();
        Ok(())
    }
}
