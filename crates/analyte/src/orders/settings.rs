//! Selects each stack's validated order settings and whether orders are sent ahead.
use crate::{Error, installation::config::Machine, invalid};
use analyte_config::{AstmOrders, Hl7Orders, LawStack};

pub(super) fn astm(machine: &Machine) -> Option<&AstmOrders> {
    machine.snapshot.effective().stack().astm_orders()
}

pub(super) fn hl7(machine: &Machine) -> Option<&Hl7Orders> {
    machine.snapshot.effective().stack().hl7_orders()
}

pub(super) fn law(machine: &Machine) -> Result<&LawStack, Error> {
    machine
        .snapshot
        .effective()
        .stack()
        .law()
        .ok_or_else(|| invalid("LAW settings absent"))
}

pub(super) fn send_ahead(machine: &Machine) -> bool {
    machine.snapshot.effective().stack().send_ahead()
}
