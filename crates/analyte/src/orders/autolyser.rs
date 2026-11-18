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
        settings: &analyte_config::AutolyserStack,
    ) -> Result<Option<analyte_autolyser::syntax::Worklist>, Error> {
        if !self
            .reserve(store, machine.machine(), |order| {
                let requests = analyte_mapping::autolyser::map_order(
                    order,
                    settings,
                    settings
                        .orders()
                        .ok_or(analyte_mapping::autolyser::OrderError::Shared(
                            analyte_mapping::OrderMappingError::Disabled,
                        ))?,
                )?;
                Ok::<_, analyte_mapping::autolyser::OrderError>(std::collections::VecDeque::from([
                    requests,
                ]))
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
            .ok_or(Error::UnexpectedInsertionOutcome)?;
        let code = analyte_dialog::autolyser::outcome::insertion(reception);
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
}
