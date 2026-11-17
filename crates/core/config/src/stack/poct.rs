//! Constructs validated POCT protocol settings.
use super::{
    CodeTables, DialogTimeouts, Error, Poct, SyntaxLimits, TcpTransport, code_tables, invalid,
    limits, tcp,
};
use crate::ParsedMachine;

/// Poct stack with its required installed policy.
#[derive(Debug, Clone)]
pub struct PoctStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<TcpTransport, SyntaxLimits, CodeTables>,
    /// Required installed protocol policy.
    pub(super) policy: Poct,
}

impl PoctStack {
    /// Protocol behavior selected by configuration and checked at load time.
    #[must_use]
    pub fn policy(&self) -> &Poct {
        &self.policy
    }
}

pub(super) fn build(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<PoctStack, Error> {
    Ok(PoctStack {
        settings: super::StackSettings {
            transport: tcp(&machine.transport, path)?,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: limits(&machine.syntax),
            codes: code_tables(machine, path)?,
        },
        policy: machine
            .poct
            .clone()
            .ok_or_else(|| invalid(path, "poct", "POCT stack requires policy"))?,
    })
}

impl PoctStack {
    /// Returns whether this selected protocol permits unsolicited work.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        false
    }
}

impl PoctStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<TcpTransport, SyntaxLimits, CodeTables> {
        &self.settings
    }
}
