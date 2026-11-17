//! Sends AUTOLYSER work through the owning machine dialog.
use super::ConnectionState;
use super::Error;
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
        let code = super::transfer_end::insertion(reception);
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

impl ConnectionState {
    pub(crate) async fn next_download(
        &mut self,
        store: &Handle,
        machine: &crate::intake::Connection<'_>,
    ) -> Result<Option<Vec<analyte_dialog::autolyser::Action>>, Error> {
        if let Some(reserved) =
            crate::orders::sending::next(store, machine, &mut self.cursor).await?
        {
            self.active = Some(reserved.order);
            let analyte_mapping::MachineOrder::Autolyser(worklist) = reserved.payload else {
                return Err(Error::ForeignStackOrder);
            };
            let actions = self.dialog.step(
                self.start.elapsed(),
                analyte_dialog::autolyser::Event::Download(&worklist),
            )?;
            Ok(Some(actions))
        } else {
            Ok(None)
        }
    }
}
