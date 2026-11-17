//! Provides inert machine hook calls with no pause state in production builds.
use super::{Error, Operation, Pause, watch};
#[derive(Default)]
pub(crate) struct MachineHooks {}
impl MachineHooks {
    pub(crate) fn enabled(&self, _: Pause) -> bool {
        false
    }
    pub(crate) async fn pause(&self, _: Pause, _: &mut watch::Receiver<bool>) -> Result<(), Error> {
        std::future::ready(Ok(())).await
    }
    pub(crate) async fn operation(&self, _: Operation) -> Result<(), Error> {
        std::future::ready(Ok(())).await
    }
    pub(crate) fn pages(&self) -> Pages {
        Pages
    }
    pub(crate) fn queued_deadline(&self, until: std::time::Instant) -> std::time::Instant {
        until
    }
    pub(crate) fn publish(&self, _: &std::sync::Arc<analyte_store::Handle>) -> Result<(), Error> {
        Ok(())
    }
}
pub(crate) struct Pages;
impl Pages {
    pub(crate) fn pause(&self) -> Result<(), Error> {
        Ok(())
    }
}
