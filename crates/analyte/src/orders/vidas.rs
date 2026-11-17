//! Reserves VIDAS work and records confirmed or uncertain transfer outcomes.
use crate::{intake, machine::vidas::Error, orders::outcome::VidasEnd};
use analyte_store::{Handle, MachineOrder};
use analyte_vidas::syntax::Message;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(crate) enum Progress {
    UploadFinished,
    DownloadNext(Message),
    DownloadFinished,
}
pub(crate) struct Work {
    active: Option<MachineOrder>,
    requests: VecDeque<Message>,
    cursor: String,
    next_poll: Instant,
}
impl Work {
    pub(crate) fn new() -> Self {
        Self {
            active: None,
            requests: VecDeque::new(),
            cursor: String::new(),
            next_poll: Instant::now(),
        }
    }
    pub(crate) fn next_poll(&self) -> Instant {
        self.next_poll
    }
    pub(crate) async fn poll(
        &mut self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
    ) -> Result<Option<Message>, Error> {
        if Instant::now() < self.next_poll {
            return Ok(None);
        }
        self.next_poll = Instant::now()
            .checked_add(Duration::from_secs(1))
            .ok_or(Error::OrderPollDeadlineOverflow)?;
        let Some(reserved) =
            super::sending::next(store, machine.machine(), &mut self.cursor).await?
        else {
            return Ok(None);
        };
        self.active = Some(reserved.order);
        let analyte_mapping::MachineOrder::Vidas(requests) = reserved.payload else {
            return Err(Error::ForeignStackOrder);
        };
        self.requests = requests.into();
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
    pub(crate) async fn close(
        &self,
        store: &Handle,
        reason: super::outcome::Transfer,
    ) -> Result<(), Error> {
        if let Some(order) = &self.active {
            super::sending::uncertain(store, order, reason).await?;
        }
        Ok(())
    }
}
