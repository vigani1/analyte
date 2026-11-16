//! Constructs validated POCT protocol settings.
use super::*;

/// Poct stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct PoctStack {
    /// Supported connection settings.
    pub(super) transport: TcpTransport,
    /// Maximum framed payload bytes.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Syntax resource ceilings.
    pub(super) limits: SyntaxLimits,
    /// Required exact test and unit translations.
    pub(super) codes: CodeTables,
    /// Required installed protocol policy.
    pub(super) policy: Poct,
}

impl PoctStack {
    /// Returns the validated transport setting.
    #[must_use]
    pub fn transport(&self) -> &TcpTransport {
        &self.transport
    }
    /// Returns the validated frame limit setting.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Returns the validated timeouts setting.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Returns the validated limits setting.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    /// Returns the validated codes setting.
    #[must_use]
    pub fn codes(&self) -> &CodeTables {
        &self.codes
    }
    /// Returns the validated policy setting.
    #[must_use]
    pub fn policy(&self) -> &Poct {
        &self.policy
    }
}

pub(super) fn build(
    machine: &MachineDocument,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<PoctStack, Error> {
    Ok(PoctStack {
        transport: tcp(&machine.transport, path)?,
        frame_limit: machine.framing.limit,
        timeouts,
        limits: limits(&machine.syntax),
        codes: code_tables(machine, path)?,
        policy: machine
            .poct
            .clone()
            .ok_or_else(|| invalid(path, "poct", "POCT stack requires policy"))?,
    })
}
