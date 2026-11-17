//! Reserves VIDAS work and records confirmed or uncertain transfer outcomes.
use crate::{
    intake,
    orders::{OrderError as Error, outcome::VidasEnd, sending::Work},
};
use analyte_store::Handle;
use analyte_vidas::syntax::Message;
use std::time::{Duration, Instant};

// Preserve the existing VIDAS idle polling cadence independently of dialog timers.
const ORDER_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) enum Progress {
    UploadFinished,
    DownloadNext(Message),
    DownloadFinished,
}
impl Work<Message> {
    pub(crate) async fn poll(
        &mut self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        settings: &analyte_config::VidasStack,
    ) -> Result<Option<Message>, Error> {
        if Instant::now() < self.next_poll {
            return Ok(None);
        }
        self.next_poll = Instant::now()
            .checked_add(ORDER_POLL_INTERVAL)
            .ok_or(Error::OrderPollDeadlineOverflow)?;
        if !self
            .reserve(store, machine.machine(), |order| {
                let requests = analyte_mapping::vidas::map_order(
                    order,
                    settings,
                    settings
                        .orders()
                        .ok_or(analyte_mapping::OrderMappingError::Disabled)?,
                )?;
                Ok(requests.into())
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
            VidasEnd::TransferConfirmed.into(),
        )
        .await?;
        self.active = None;
        Ok(Progress::DownloadFinished)
    }
}
