//! Holds validated size and time limits and converts raw settings.
use super::Syntax;

/// Receive and send deadlines shared by the host dialogs.
/// Cloning validated settings never grants write access.
/// ```compile_fail
/// fn invalidate(settings: &analyte_config::DialogTimeouts) {
///     let mut settings = settings.clone();
///     settings.receive_timeout_ms = 0;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DialogTimeouts {
    /// Receive deadline.
    pub(super) receive_timeout_ms: u32,
    /// Send deadline.
    pub(super) send_timeout_ms: u32,
}

/// Explicit syntax resource ceilings retained from the config.
/// ```compile_fail
/// fn invalidate(limits: &analyte_config::SyntaxLimits) {
///     let mut limits = limits.clone();
///     limits.bytes = 0;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SyntaxLimits {
    /// Total message bytes.
    pub(super) bytes: usize,
    /// Maximum records or segments.
    pub(super) records: usize,
    /// Maximum record bytes.
    pub(super) record_bytes: usize,
    /// Maximum fields.
    pub(super) fields: usize,
    /// Maximum repetitions.
    pub(super) repetitions: usize,
    /// Maximum components.
    pub(super) components: usize,
}

pub(super) fn limits(syntax: &Syntax) -> SyntaxLimits {
    SyntaxLimits {
        bytes: syntax.bytes,
        records: syntax.records,
        record_bytes: syntax.record_bytes,
        fields: syntax.fields,
        repetitions: syntax.repetitions,
        components: syntax.components,
    }
}

impl DialogTimeouts {
    /// Maximum receive wait in milliseconds.
    #[must_use]
    pub fn receive_timeout_ms(&self) -> u32 {
        self.receive_timeout_ms
    }
    /// Maximum send wait in milliseconds.
    #[must_use]
    pub fn send_timeout_ms(&self) -> u32 {
        self.send_timeout_ms
    }
}

impl SyntaxLimits {
    /// Maximum message bytes accepted before refusing further input.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Maximum records accepted in one message.
    #[must_use]
    pub fn records(&self) -> usize {
        self.records
    }
    /// Maximum bytes accepted in one record.
    #[must_use]
    pub fn record_bytes(&self) -> usize {
        self.record_bytes
    }
    /// Maximum fields accepted in one segment.
    #[must_use]
    pub fn fields(&self) -> usize {
        self.fields
    }
    /// Maximum repetitions accepted in one field.
    #[must_use]
    pub fn repetitions(&self) -> usize {
        self.repetitions
    }
    /// Maximum components accepted in one field.
    #[must_use]
    pub fn components(&self) -> usize {
        self.components
    }
}
