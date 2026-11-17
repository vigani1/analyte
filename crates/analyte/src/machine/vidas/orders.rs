//! Sends VIDAS work through the owning machine dialog.
use super::ConnectionState;
use super::Error;
use super::transfer_end::VidasEnd;
use crate::intake;
use crate::machine::custody::ms;
use analyte_dialog::vidas::Dialog;
use analyte_store::Handle;
use std::time::Instant;
impl ConnectionState {
    pub(crate) async fn poll_orders(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
    ) -> Result<(), Error> {
        if self.idle && self.opened && Instant::now() >= self.next_poll {
            self.next_poll = Instant::now()
                .checked_add(ms(1000))
                .ok_or_else(|| Error::OrderPollDeadlineOverflow)?;
            if let Some(reserved) =
                crate::orders::sending::next(store, machine.machine(), &mut self.cursor).await?
            {
                self.active = Some(reserved.order);
                let analyte_mapping::MachineOrder::Vidas(requests) = reserved.payload else {
                    return Err(Error::ForeignStackOrder);
                };
                self.requests = requests.into();
                start_download(self)?;
            }
        }
        Ok(())
    }
}
pub(crate) fn start_download(state: &mut ConnectionState) -> Result<(), Error> {
    let request = state
        .requests
        .pop_front()
        .ok_or_else(|| Error::MappedRequestMissing)?;
    state.dialog = Dialog::download(state.config.clone(), state.policy.clone(), &request)?;
    state.start = Instant::now();
    state.idle = false;
    state.end_code = VidasEnd::TransferError;
    state.actions.extend(state.dialog.start());
    Ok(())
}
