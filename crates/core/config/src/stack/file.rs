//! Constructs validated FILE protocol settings.
use super::RecordKind;
use super::{
    AstmOrders, AstmSyntax, DialogTimeouts, Error, Hl7Orders, Hl7Syntax, Mappings, RecordTransport,
    Stack, astm_orders, astm_syntax, hl7_orders, hl7_syntax, invalid,
};
use crate::ParsedMachine;

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileAstmStack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<u32, AstmSyntax, Mappings>,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub(super) orders: Option<AstmOrders>,
}

/// Watched-folder records without any stream framing or acknowledgment policy.
#[derive(Debug, Clone)]
pub struct FileHl7Stack {
    /// Shared link, resource and interpretation settings.
    pub(super) settings: super::StackSettings<u32, Hl7Syntax, Mappings>,
    /// Retained configured outbound policy; file pickup performs no machine sends.
    pub(super) orders: Option<Hl7Orders>,
}

pub(super) fn file_stack(
    machine: &ParsedMachine<'_>,
    timeouts: DialogTimeouts,
    path: &str,
) -> Result<Stack, Error> {
    Ok(match machine.choice().file_record_kind() {
        Some(RecordKind::Astm) => Stack::Astm(RecordTransport::Folder(Box::new(FileAstmStack {
            settings: super::StackSettings {
                transport: machine.transport.operation_timeout_ms,
                frame_limit: machine.framing.limit,
                timeouts,
                syntax: astm_syntax(&machine.syntax),
                codes: machine
                    .mappings
                    .clone()
                    .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
            },
            orders: machine
                .orders
                .as_ref()
                .map(|o| astm_orders(o, path))
                .transpose()?,
        }))),
        Some(RecordKind::Hl7) => Stack::Hl7(RecordTransport::Folder(Box::new(FileHl7Stack {
            settings: super::StackSettings {
                transport: machine.transport.operation_timeout_ms,
                frame_limit: machine.framing.limit,
                timeouts,
                syntax: hl7_syntax(&machine.syntax, path)?,
                codes: machine
                    .mappings
                    .clone()
                    .ok_or_else(|| invalid(path, "mappings", "record stack requires mappings"))?,
            },
            orders: machine
                .orders
                .as_ref()
                .map(|o| hl7_orders(o, path))
                .transpose()?,
        }))),
        None | Some(RecordKind::Vendor) => {
            return Err(Error::IncompatibleStack {
                path: path.into(),
                reason: "validated file pickup supports only LIS02 or HL7",
            });
        }
    })
}

impl FileAstmStack {
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&AstmOrders> {
        self.orders.as_ref()
    }
}

impl FileHl7Stack {
    /// Validated order policy used before encoding commands.
    #[must_use]
    pub fn orders(&self) -> Option<&Hl7Orders> {
        self.orders.as_ref()
    }
}

impl FileAstmStack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<u32, AstmSyntax, Mappings> {
        &self.settings
    }
}

impl FileHl7Stack {
    /// Borrows the shared validated settings without changing protocol-specific types.
    #[must_use]
    pub fn settings(&self) -> &super::StackSettings<u32, Hl7Syntax, Mappings> {
        &self.settings
    }
}
