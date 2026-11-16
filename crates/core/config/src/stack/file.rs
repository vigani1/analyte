//! Constructs validated FILE protocol settings.
use super::*;

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileAstmStack {
    /// External operation deadline.
    pub(super) operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Record syntax settings.
    pub(super) syntax: AstmSyntax,
    /// Required field selectors and code tables.
    pub(super) mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub(super) orders: Option<AstmOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileHl7Stack {
    /// External operation deadline.
    pub(super) operation_timeout_ms: u32,
    /// Configured file payload ceiling.
    pub(super) frame_limit: usize,
    /// Host receive and send deadlines.
    pub(super) timeouts: DialogTimeouts,
    /// Record syntax settings.
    pub(super) syntax: Hl7Syntax,
    /// Required field selectors and code tables.
    pub(super) mappings: Mappings,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub(super) orders: Option<Hl7Orders>,
}

#[expect(
    clippy::expect_used,
    reason = "private conversion runs only after validate_machine proves required sections and options"
)]
pub(super) fn file_stack(machine: &MachineDocument, timeouts: DialogTimeouts) -> Stack {
    match machine.syntax.kind {
        SyntaxKind::AstmLis02 => Stack::Astm(RecordTransport::Folder(Box::new(FileAstmStack {
            operation_timeout_ms: machine.transport.operation_timeout_ms,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: astm_syntax(&machine.syntax),
            mappings: machine
                .mappings
                .clone()
                .expect("validated record stack requires mappings"),
            orders: machine.orders.as_ref().map(astm_orders),
        }))),
        SyntaxKind::Hl7V2 => Stack::Hl7(RecordTransport::Folder(Box::new(FileHl7Stack {
            operation_timeout_ms: machine.transport.operation_timeout_ms,
            frame_limit: machine.framing.limit,
            timeouts,
            syntax: hl7_syntax(&machine.syntax),
            mappings: machine
                .mappings
                .clone()
                .expect("validated record stack requires mappings"),
            orders: machine.orders.as_ref().map(hl7_orders),
        }))),
        SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser => {
            unreachable!("validated file pickup supports only LIS02 or HL7")
        }
    }
}

impl FileAstmStack {
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
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
    /// Returns the validated syntax setting.
    #[must_use]
    pub fn syntax(&self) -> &AstmSyntax {
        &self.syntax
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<AstmOrders> {
        &self.orders
    }
}

impl FileHl7Stack {
    /// Returns the validated operation timeout ms setting.
    #[must_use]
    pub fn operation_timeout_ms(&self) -> u32 {
        self.operation_timeout_ms
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
    /// Returns the validated syntax setting.
    #[must_use]
    pub fn syntax(&self) -> &Hl7Syntax {
        &self.syntax
    }
    /// Returns the validated mappings setting.
    #[must_use]
    pub fn mappings(&self) -> &Mappings {
        &self.mappings
    }
    /// Returns the validated orders setting.
    #[must_use]
    pub fn orders(&self) -> &Option<Hl7Orders> {
        &self.orders
    }
}
