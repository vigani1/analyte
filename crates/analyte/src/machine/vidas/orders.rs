//! Reserves VIDAS work and records confirmed or uncertain transfer outcomes.
use crate::{intake, machine::vidas::Error, orders::sending::Work};
use analyte_store::Handle;
use analyte_vidas::syntax::Message;
pub(crate) enum Progress {
    UploadFinished,
    DownloadFinished,
}
impl Work<'_, analyte_config::VidasOrders, Vec<Message>> {
    pub(crate) async fn poll(
        &mut self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        settings: &analyte_config::VidasStack,
    ) -> Result<Option<Vec<Message>>, Error> {
        self.poll_one(store, machine.machine(), |order, orders| {
            Ok(analyte_mapping::vidas::map_order(order, settings, orders)?)
        })
        .await
    }
    pub(crate) async fn confirmed(&mut self, store: &Handle) -> Result<Progress, Error> {
        let Some(order) = &self.active else {
            return Ok(Progress::UploadFinished);
        };
        crate::orders::sending::complete(
            store,
            order,
            analyte_model::OrderState::Sent,
            analyte_dialog::vidas::outcome::Code::TransferConfirmed.as_str(),
        )
        .await?;
        self.active = None;
        Ok(Progress::DownloadFinished)
    }
}
