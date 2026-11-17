//! Reserves Autolyser work and reconciles insertion replies without automatic replay.
use crate::{
    intake,
    machine::autolyser::{Error, transfer_end},
};
use analyte_store::{Handle, MachineOrder};
#[derive(Default)]
pub(crate) struct Work {
    active: Option<MachineOrder>,
    cursor: String,
}
impl Work {
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) async fn poll(
        &mut self,
        store: &Handle,
        machine: &intake::Connection<'_>,
    ) -> Result<Option<analyte_autolyser::syntax::Worklist>, Error> {
        let Some(reserved) =
            super::sending::next(store, machine.machine(), &mut self.cursor).await?
        else {
            return Ok(None);
        };
        self.active = Some(reserved.order);
        let analyte_mapping::MachineOrder::Autolyser(worklist) = reserved.payload else {
            return Err(Error::ForeignStackOrder);
        };
        Ok(Some(worklist))
    }
    pub(crate) async fn insertion(
        &mut self,
        reception: analyte_dialog::autolyser::Reception,
        store: &Handle,
    ) -> Result<bool, Error> {
        let order = self
            .active
            .as_ref()
            .ok_or(Error::UnexpectedAutolyserInsertionOutcome)?;
        let code = transfer_end::insertion(reception);
        let outcome = match reception {
            analyte_dialog::autolyser::Reception::Accepted(_) => analyte_model::OrderState::Sent,
            analyte_dialog::autolyser::Reception::Rejected(_)
            | analyte_dialog::autolyser::Reception::Other(_, _) => {
                analyte_model::OrderState::Uncertain
            }
        };
        super::sending::complete(store, order, outcome, &code).await?;
        self.active = None;
        Ok(outcome == analyte_model::OrderState::Sent)
    }
    pub(crate) async fn close(&self, store: &Handle, reason: &str) -> Result<(), Error> {
        if let Some(order) = &self.active {
            super::sending::uncertain(store, order, reason).await?;
        }
        Ok(())
    }
    pub(crate) async fn refuse(&mut self, store: &Handle, reason: &str) -> Result<(), Error> {
        self.close(store, reason).await?;
        self.active = None;
        Ok(())
    }
}
