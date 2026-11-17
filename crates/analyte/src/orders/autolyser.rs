//! Sends AUTOLYSER work through the owning machine dialog.
use crate::machine::autolyser::ConnectionState;
use crate::machine::error::SessionError as Error;
use analyte_store::Handle;
impl ConnectionState {
    pub(crate) async fn complete_insertion(
        &mut self,
        reception: analyte_dialog::autolyser::Reception,
        store: &Handle,
    ) -> Result<bool, Error> {
        self.receipt.take();
        let order = self
            .active
            .as_ref()
            .ok_or_else(|| Error::UnexpectedAutolyserInsertionOutcome)?;
        let code = crate::machine::transfer_end::insertion(reception);
        let outcome = match reception {
            analyte_dialog::autolyser::Reception::Accepted(_) => analyte_model::OrderState::Sent,
            analyte_dialog::autolyser::Reception::Rejected(_)
            | analyte_dialog::autolyser::Reception::Other(_, _) => {
                analyte_model::OrderState::Uncertain
            }
        };
        crate::orders::sending::complete(store, order, outcome, &code).await?;
        self.active = None;
        self.ready = outcome == analyte_model::OrderState::Sent;

        Ok(!self.ready)
    }
}
