//! Capability admission follows the validated stack, never independent section presence.
use crate::{Error, config::Machine, invalid};
use analyte_config::{AstmOrders, Hl7Orders, Hl7Syntax, LawStack, Stack};

pub(super) fn astm(machine: &Machine) -> Option<&AstmOrders> {
    match machine.snapshot.effective().stack() {
        Stack::Astm(s) => s.orders.as_ref(),
        Stack::FileAstm(s) => s.orders.as_ref(),
        Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::FileHl7(_) => None,
    }
}

pub(super) struct Hl7<'a> {
    pub(super) syntax: &'a Hl7Syntax,
    pub(super) orders: &'a Hl7Orders,
}

pub(super) fn hl7(machine: &Machine) -> Option<Hl7<'_>> {
    match machine.snapshot.effective().stack() {
        Stack::Hl7(s) => s.orders.as_ref().map(|orders| Hl7 {
            syntax: &s.syntax,
            orders,
        }),
        Stack::FileHl7(s) => s.orders.as_ref().map(|orders| Hl7 {
            syntax: &s.syntax,
            orders,
        }),
        Stack::Astm(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::FileAstm(_) => None,
    }
}

pub(super) fn law(machine: &Machine) -> Result<&LawStack, Error> {
    match machine.snapshot.effective().stack() {
        Stack::Law(s) => Ok(s),
        Stack::Astm(_)
        | Stack::Hl7(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::FileAstm(_)
        | Stack::FileHl7(_) => Err(invalid("LAW settings absent")),
    }
}

pub(super) fn send_ahead(machine: &Machine) -> bool {
    match machine.snapshot.effective().stack() {
        Stack::Astm(s) => s.orders.as_ref().is_some_and(|o| o.send_ahead),
        Stack::FileAstm(s) => s.orders.as_ref().is_some_and(|o| o.send_ahead),
        Stack::Hl7(s) => s.orders.as_ref().is_some_and(|o| o.send.is_some()),
        Stack::FileHl7(s) => s.orders.as_ref().is_some_and(|o| o.send.is_some()),
        Stack::Law(s) => s.orders.as_ref().is_some_and(|o| o.send_ahead),
        Stack::VidasLiteral(s) => s.orders.as_ref().is_some_and(|o| o.send_ahead),
        Stack::Autolyser(s) => s.orders.as_ref().is_some_and(|o| o.send_ahead),
        Stack::Poct(_) => false,
    }
}
