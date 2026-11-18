//! Reserves VIDAS work and records confirmed or uncertain transfer outcomes.
use crate::{
    intake,
    orders::{OrderError as Error, sending::Work},
};
use analyte_store::Handle;
use analyte_vidas::syntax::Message;
pub(crate) enum Progress {
    UploadFinished,
    DownloadNext(Message),
    DownloadFinished,
}
impl Work<'_, analyte_config::VidasOrders, Message> {
    pub(crate) async fn poll(
        &mut self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        settings: &analyte_config::VidasStack,
    ) -> Result<Option<Message>, Error> {
        if !self.poll_due()? {
            return Ok(None);
        }
        if !self
            .reserve(store, machine.machine(), |order, orders| {
                let requests = analyte_mapping::vidas::map_order(order, settings, orders)?;
                Ok::<_, analyte_mapping::vidas::OrderError>(requests.into())
            })
            .await?
        {
            return Ok(None);
        }
        Ok(Some(
            self.requests
                .pop_front()
                .ok_or(Error::MappedRequestMissing)?,
        ))
    }
    pub(crate) async fn confirmed(&mut self, store: &Handle) -> Result<Progress, Error> {
        let Some(order) = &self.active else {
            return Ok(Progress::UploadFinished);
        };
        if let Some(request) = self.requests.pop_front() {
            return Ok(Progress::DownloadNext(request));
        }
        super::sending::complete(
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
