//! Holds validated size and time limits and converts raw settings.
use super::{Encoding, Escapes, Syntax, Terminator};

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

pub(super) fn hl7_options(
    encoding: Encoding,
    terminator: Terminator,
    escapes: Escapes,
    limits: &SyntaxLimits,
) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: false,
        charset: match encoding {
            Encoding::Ascii => analyte_hl7::Charset::Ascii,
            Encoding::Latin1 => analyte_hl7::Charset::Latin1,
            Encoding::Utf8 => analyte_hl7::Charset::Utf8,
        },
        terminator: match terminator {
            Terminator::Cr => analyte_hl7::Terminator::Cr,
            Terminator::CrLf => analyte_hl7::Terminator::CrLf,
            Terminator::Lf => analyte_hl7::Terminator::Lf,
        },
        escapes: match escapes {
            Escapes::Reject => analyte_hl7::EscapePolicy::Reject,
            Escapes::Preserve => analyte_hl7::EscapePolicy::Preserve,
        },
        limits: analyte_hl7::Limits {
            message_bytes: limits.bytes,
            segments: limits.records,
            fields: limits.fields,
            repetitions: limits.repetitions,
        },
    }
}

impl DialogTimeouts {
    /// Returns the validated receive timeout ms setting.
    #[must_use]
    pub fn receive_timeout_ms(&self) -> u32 {
        self.receive_timeout_ms
    }
    /// Returns the validated send timeout ms setting.
    #[must_use]
    pub fn send_timeout_ms(&self) -> u32 {
        self.send_timeout_ms
    }
}

impl SyntaxLimits {
    /// Returns the validated bytes setting.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Returns the validated records setting.
    #[must_use]
    pub fn records(&self) -> usize {
        self.records
    }
    /// Returns the validated record bytes setting.
    #[must_use]
    pub fn record_bytes(&self) -> usize {
        self.record_bytes
    }
    /// Returns the validated fields setting.
    #[must_use]
    pub fn fields(&self) -> usize {
        self.fields
    }
    /// Returns the validated repetitions setting.
    #[must_use]
    pub fn repetitions(&self) -> usize {
        self.repetitions
    }
    /// Returns the validated components setting.
    #[must_use]
    pub fn components(&self) -> usize {
        self.components
    }
}
