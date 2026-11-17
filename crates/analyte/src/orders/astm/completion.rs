//! Settles ASTM transport completion without implying application acceptance.
use crate::orders::{OrderError as Error, outcome::Code, sending::Work};
use analyte_model::OrderState;
use analyte_store::{Handle, MachineOrder};

/// Records a specific failed transfer as uncertain; normal transfer becomes Sent.
pub(crate) async fn finished(
    store: &Handle,
    order: &MachineOrder,
    end: analyte_dialog::astm::End,
) -> Result<(), Error> {
    // LIS01 confirms transfer, not the analyzer application's acceptance.
    // Only explicit application evidence may advance Sent to Accepted.
    let code = match end {
        analyte_dialog::astm::End::Normal => Code::AstmTransferConfirmed,
        analyte_dialog::astm::End::Abort => Code::AstmTransferAborted,
        analyte_dialog::astm::End::Timeout => Code::AstmTransferTimeout,
        analyte_dialog::astm::End::Interrupt => Code::AstmTransferInterrupted,
        analyte_dialog::astm::End::Incomplete => Code::AstmTransferIncomplete,
    };
    let state = if end == analyte_dialog::astm::End::Normal {
        OrderState::Sent
    } else {
        OrderState::Uncertain
    };
    super::super::sending::complete(store, order, state, code.into()).await
}

impl<T> Work<T> {
    /// LIS01 confirms transport only; application acceptance remains separate.
    pub(crate) async fn astm_finished(
        &mut self,
        store: &Handle,
        end: analyte_dialog::astm::End,
    ) -> Result<(), Error> {
        if let Some(order) = &self.active {
            finished(store, order, end).await?;
        }
        self.active = None;
        Ok(())
    }
}
