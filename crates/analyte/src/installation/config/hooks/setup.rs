//! Installs synthetic pause gates and loopback overrides through explicit test APIs.
use super::super::{Error, Machine, Program};
use std::time::Duration;
impl Program {
    /// Overrides delivery/poller store waits for a synthetic loopback destination.
    /// Production entry points reject the required destination override.
    /// # Errors
    /// Rejects missing loopback overrides and budgets outside 1 through 60 seconds.
    pub fn set_loopback_store_budget_for_test(
        &mut self,
        name: &str,
        budget: Duration,
    ) -> Result<(), Error> {
        if !self.hooks.loopback_destinations.contains_key(name)
            || !(Duration::from_secs(1)..=Duration::from_secs(60)).contains(&budget)
        {
            return Err(Error::InvalidTestStoreBudget);
        }
        for machine in &mut self.machines {
            if machine
                .snapshot
                .effective()
                .site()
                .destination
                .name
                .as_str()
                == name
            {
                machine.destination_limits.store_timeout = budget;
            }
        }
        for kept in &mut self.retired {
            if kept.settings.name.as_str() == name {
                kept.limits.store_timeout = budget;
            }
        }
        Ok(())
    }

    /// Pauses after an Autolyser exchange commits, before another poll.
    /// # Errors
    /// Rejects unknown machines and non-injected listeners.
    pub fn pause_autolyser_quiescent_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self.test_listener_machine_mut(id)?;
        machine.hooks.autolyser_quiescent = Some(hook);
        Ok(())
    }
    fn test_listener_machine_mut(&mut self, id: &str) -> Result<&mut Machine, Error> {
        let machine = self.test_machine_mut(id)?;
        if machine.listener.is_none() {
            return Err(Error::TestListenerMismatch);
        }
        Ok(machine)
    }
    fn test_machine_mut(&mut self, id: &str) -> Result<&mut Machine, Error> {
        self.machines
            .iter_mut()
            .find(|machine| machine.id.as_str() == id)
            .ok_or_else(|| Error::UnknownTestMachine)
    }
    /// Installs a one-shot LAW raw-commit/pre-response pause on an injected loopback machine.
    /// # Errors
    /// Rejects unknown machines, non-LAW stacks and production listeners.
    pub fn pause_law_committed_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self.test_machine_mut(id)?;
        if machine.listener.is_none()
            || !matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Law(_)
            )
        {
            return Err(Error::MissingLawCommitListener);
        }
        machine.hooks.law_committed = Some(hook);
        Ok(())
    }
    /// Installs LAW evidence gates on an injected loopback listener.
    /// # Errors
    /// Rejects non-LAW or non-test machines.
    pub fn install_law_gates_for_test(
        &mut self,
        id: &str,
        gates: crate::runtime::test_hooks::LawGates,
    ) -> Result<(), Error> {
        let machine = self.test_machine_mut(id)?;
        if machine.listener.is_none()
            || !matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Law(_)
            )
        {
            return Err(Error::MissingLawGateListener);
        }
        machine.hooks.law_gates = gates;
        Ok(())
    }
    /// Installs a one-shot pause immediately before LAW raw custody, on a test listener.
    /// # Errors
    /// Rejects the same non-loopback and non-LAW machines as the commit hook.
    pub fn pause_law_precommit_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        self.pause_law_committed_for_test(id, hook.clone())?;
        let machine = self.test_machine_mut(id)?;
        machine.hooks.law_committed = None;
        machine.hooks.law_precommit = Some(hook);
        Ok(())
    }
    /// Stalls the store at shutdown with a 500 ms process deadline, for child-process tests only.
    /// This deliberately never returns from a storage operation; the watchdog must exit 3.
    /// # Errors
    /// Requires injected loopback listeners for every machine; production entry points refuse it.
    pub fn stall_store_on_shutdown_for_test(&mut self) -> Result<(), Error> {
        if self.machines.is_empty()
            || self
                .machines
                .iter()
                .any(|machine| machine.listener.is_none())
        {
            return Err(Error::MissingShutdownListeners);
        }
        self.hooks.stall_store_on_shutdown = true;
        Ok(())
    }
    /// Installs a one-shot ASTM durable-frame/pre-ACK pause for a loopback test machine.
    /// # Errors
    /// Rejects unknown machines, non-ASTM stacks, or machines without an injected loopback listener.
    pub fn pause_astm_committed_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self.test_machine_mut(id)?;
        if machine.listener.is_none()
            || !matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
            )
        {
            return Err(Error::MissingAstmCommitListener);
        }
        machine.hooks.astm_committed = Some(hook);
        Ok(())
    }
    /// Pauses every ASTM stream machine of a real `run` once, after a frame is durably
    /// stored and before its ACK, and writes `marker` there so a test in another process
    /// can kill the program at that point. Keep the returned control alive while serving;
    /// a pause nobody kills ends after one minute by stopping the connection, never by
    /// acknowledging.
    /// # Errors
    /// Rejects a program without an ASTM stream machine.
    pub fn pause_astm_committed_with_marker(
        &mut self,
        marker: std::path::PathBuf,
    ) -> Result<crate::runtime::test_hooks::Control, Error> {
        let (hook, control) =
            crate::runtime::test_hooks::Committed::with_marker(marker, Duration::from_secs(60));
        let mut installed = false;
        for machine in &mut self.machines {
            if matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
            ) {
                machine.hooks.astm_committed = Some(hook.clone());
                installed = true;
            }
        }
        if installed {
            Ok(control)
        } else {
            Err(Error::MissingAstmPauseMachine)
        }
    }
    /// Pauses a loopback client's closing owner after protocol cleanup, before transport release.
    /// # Errors
    /// Rejects unknown machines and endpoints that are not loopback TCP clients.
    pub fn pause_connection_closing_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self.test_machine_mut(id)?;
        if machine.snapshot.effective().stack().transport_kind()
            != analyte_config::TransportKind::TcpClient
            || !machine
                .snapshot
                .effective()
                .site()
                .endpoint
                .fixed()
                .map_err(|_| Error::InvalidClosingHookTransport)?
                .parse::<std::net::SocketAddr>()?
                .ip()
                .is_loopback()
        {
            return Err(Error::InvalidClosingHookTransport);
        }
        machine.hooks.connection_closing = Some(hook);
        Ok(())
    }
    /// Supplies an already bound loopback listener for deterministic integration tests.
    /// The caller can retain a clone to reserve the same address across restarts.
    /// # Errors
    /// Rejects unknown machines, non-loopback addresses and non-server configs.
    pub fn set_loopback_listener_for_test(
        &mut self,
        id: &str,
        listener: std::net::TcpListener,
    ) -> Result<(), Error> {
        let machine = self.test_machine_mut(id)?;
        if !listener.local_addr()?.ip().is_loopback()
            || machine.snapshot.effective().stack().transport_kind()
                != analyte_config::TransportKind::TcpServer
            || machine.snapshot.effective().site().endpoint.fixed()
                != Ok(listener.local_addr()?.to_string().as_str())
        {
            return Err(Error::TestListenerMismatch);
        }
        listener.set_nonblocking(true)?;
        machine.listener = Some(listener);
        Ok(())
    }
    /// Overrides a known destination for synthetic loopback HTTP tests only.
    /// Production `serve` and `run` reject programs containing these overrides.
    /// # Errors
    /// Rejects unknown destinations; the test service validates literal loopback URLs.
    pub fn set_loopback_destination_for_test(
        &mut self,
        name: &str,
        url: String,
    ) -> Result<(), Error> {
        if !self
            .machines
            .iter()
            .any(|m| m.snapshot.effective().site().destination.name.as_str() == name)
            && !self
                .retired
                .iter()
                .any(|kept| kept.settings.name.as_str() == name)
        {
            return Err(Error::UnknownTestDestination);
        }
        self.hooks.loopback_destinations.insert(name.into(), url);
        Ok(())
    }
    /// Shortens feed polling for an already overridden synthetic destination.
    /// Production entry points reject the required loopback destination override.
    /// # Errors
    /// Rejects destinations without an override and intervals outside 1 ms to 5 s.
    pub fn set_loopback_poll_interval_for_test(
        &mut self,
        name: &str,
        interval: Duration,
    ) -> Result<(), Error> {
        if !self.hooks.loopback_destinations.contains_key(name)
            || !(Duration::from_millis(1)..=Duration::from_secs(5)).contains(&interval)
        {
            return Err(Error::InvalidTestPolling);
        }
        for machine in &mut self.machines {
            if machine
                .snapshot
                .effective()
                .site()
                .destination
                .name
                .as_str()
                == name
            {
                machine.destination_limits.poll_interval = interval;
            }
        }
        // A kept destination has no poller; a short interval here makes any poll
        // that wrongly started for it show up at once in a test.
        for kept in &mut self.retired {
            if kept.settings.name.as_str() == name {
                kept.limits.poll_interval = interval;
            }
        }
        Ok(())
    }

    /// Pauses after one input chunk has entered the protocol dialog.
    /// # Errors
    /// Rejects unknown machines and non-injected listeners.
    pub fn pause_input_processed_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::runtime::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self.test_listener_machine_mut(id)?;
        machine.hooks.input_processed = Some(hook);
        Ok(())
    }
}
