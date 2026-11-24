//! Owns feature-enabled machine pause state and order/store gates.
use super::{Error, Operation, Pause, watch};
#[derive(Default)]
pub(crate) struct MachineHooks {
    pub(crate) astm_frame_budget: Option<std::time::Duration>,
    pub(crate) autolyser_quiescent: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
    pub(crate) input_processed: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
    pub(crate) law_gates: crate::runtime::test_hooks::LawGates,
    pub(crate) law_precommit: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
    pub(crate) law_committed: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
    pub(crate) astm_committed: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
    pub(crate) connection_closing: Option<std::sync::Arc<crate::runtime::test_hooks::Committed>>,
}
impl MachineHooks {
    pub(crate) fn astm_frame_budget(&self) -> Option<std::time::Duration> {
        self.astm_frame_budget
    }
    pub(crate) fn enabled(&self, point: Pause) -> bool {
        self.gate(point).is_some()
    }
    fn gate(&self, point: Pause) -> Option<&crate::runtime::test_hooks::Committed> {
        match point {
            Pause::AutolyserQuiescent => self.autolyser_quiescent.as_deref(),
            Pause::InputProcessed => self.input_processed.as_deref(),
            Pause::LawPrecommit => self.law_precommit.as_deref(),
            Pause::LawCommitted => self.law_committed.as_deref(),
            Pause::AstmCommitted => self.astm_committed.as_deref(),
            Pause::ConnectionClosing => self.connection_closing.as_deref(),
            Pause::LawRsp => self.law_gates.rsp.as_deref(),
            Pause::LawHandover => self.law_gates.handover.as_deref(),
            Pause::LawQueued => self.law_gates.queued.as_deref(),
        }
    }
    pub(crate) async fn pause(
        &self,
        point: Pause,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        if let Some(gate) = self.gate(point) {
            gate.pause(stop).await?;
        }
        Ok(())
    }
    pub(crate) async fn operation(&self, point: Operation) -> Result<(), Error> {
        let gate = match point {
            Operation::Lookup => &self.law_gates.lookup,
            Operation::Reserved => &self.law_gates.reserved,
            Operation::Cancel => &self.law_gates.cancel,
            Operation::CancelResult => &self.law_gates.cancel_result,
        };
        if let Some(gate) = gate {
            gate.operation().await?;
        }
        Ok(())
    }
    pub(crate) fn pages(&self) -> Pages {
        Pages(self.law_gates.pages.clone())
    }
    pub(crate) fn queued_deadline(&self, until: std::time::Instant) -> std::time::Instant {
        if self.law_gates.expire_queued {
            std::time::Instant::now()
        } else {
            until
        }
    }
    pub(crate) fn publish(
        &self,
        store: &std::sync::Arc<analyte_store::Handle>,
    ) -> Result<(), Error> {
        if let Some(probe) = &self.law_gates.store {
            probe.publish(store)?;
        }
        Ok(())
    }
}
pub(crate) struct Pages(Option<std::sync::Arc<crate::runtime::test_hooks::StoreGate>>);
impl Pages {
    pub(crate) fn pause(&self) -> Result<(), Error> {
        if let Some(gate) = &self.0 {
            gate.pause()?;
        }
        Ok(())
    }
}
