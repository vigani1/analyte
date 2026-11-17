//! Reserves Autolyser work and reconciles insertion replies without automatic replay.
use crate::{
    intake,
    orders::{OrderError as Error, sending::Work},
};
use analyte_store::Handle;
impl Work<analyte_autolyser::syntax::Worklist> {
    pub(crate) async fn poll(
        &mut self,
        store: &Handle,
        machine: &intake::ConnectionEvidence<'_>,
    ) -> Result<Option<analyte_autolyser::syntax::Worklist>, Error> {
        if !self
            .reserve(store, machine.machine(), |payload| {
                let analyte_mapping::MachineOrder::Autolyser(worklist) = payload else {
                    return Err(Error::ForeignStackOrder);
                };
                Ok(std::collections::VecDeque::from([worklist]))
            })
            .await?
        {
            return Ok(None);
        }
        Ok(self.requests.pop_front())
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
        let code = super::outcome::Transfer::AutolyserInsertion(reception);
        let outcome = match reception {
            analyte_dialog::autolyser::Reception::Accepted(_) => analyte_model::OrderState::Sent,
            analyte_dialog::autolyser::Reception::Rejected(_)
            | analyte_dialog::autolyser::Reception::Other(_, _) => {
                analyte_model::OrderState::Uncertain
            }
        };
        super::sending::complete(store, order, outcome, code).await?;
        self.active = None;
        Ok(outcome == analyte_model::OrderState::Sent)
    }
}
