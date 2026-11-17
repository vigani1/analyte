//! Shares validated link settings and typed interpretation settings across protocol stacks.
use super::{AstmSyntax, DialogTimeouts, Hl7Syntax, SyntaxLimits};

/// Typed settings shared by stream and watched-folder protocol stacks.
/// Transport and interpretation types retain each protocol's supported combinations.
#[derive(Debug, Clone)]
pub struct StackSettings<T, S, C> {
    pub(super) transport: T,
    pub(super) frame_limit: usize,
    pub(super) timeouts: DialogTimeouts,
    pub(super) syntax: S,
    pub(super) codes: C,
}

impl<T, S, C> StackSettings<T, S, C> {
    /// Borrows the selected physical transport settings.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }
    /// Maximum retained frame or file payload size.
    #[must_use]
    pub fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// Host receive and send deadlines.
    #[must_use]
    pub fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
    /// Protocol-specific grammar and encoding settings.
    #[must_use]
    pub fn syntax(&self) -> &S {
        &self.syntax
    }
    /// Explicit source mappings, including selectors for record protocols.
    #[must_use]
    pub fn codes(&self) -> &C {
        &self.codes
    }
}

/// Supplies resource ceilings from the selected syntax settings.
/// Implemented by the workspace's two configurable grammars and fixed grammars.
pub trait SyntaxBounds {
    /// Borrows the ceilings used before parsing untrusted input.
    fn syntax_limits(&self) -> &SyntaxLimits;
}
impl SyntaxBounds for AstmSyntax {
    fn syntax_limits(&self) -> &SyntaxLimits {
        self.limits()
    }
}
impl SyntaxBounds for Hl7Syntax {
    fn syntax_limits(&self) -> &SyntaxLimits {
        self.limits()
    }
}
impl SyntaxBounds for SyntaxLimits {
    fn syntax_limits(&self) -> &SyntaxLimits {
        self
    }
}
impl<T, S: SyntaxBounds, C> StackSettings<T, S, C> {
    /// Size and count ceilings applied before parsing untrusted input.
    #[must_use]
    pub fn limits(&self) -> &SyntaxLimits {
        self.syntax.syntax_limits()
    }
}
