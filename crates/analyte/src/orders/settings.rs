//! Capability admission follows the validated stack, never independent section presence.
use crate::{Error, config::Machine, invalid};
use analyte_config::{AstmOrders, Hl7Orders, LawStack, Stack};

pub(super) fn astm(machine: &Machine) -> Option<&AstmOrders> {
    match machine.snapshot.effective().stack() {
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => s.orders().as_ref(),
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => s.orders().as_ref(),
        Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => None,
    }
}

pub(super) fn hl7(machine: &Machine) -> Option<&Hl7Orders> {
    match machine.snapshot.effective().stack() {
        Stack::Hl7(analyte_config::RecordTransport::Stream(s)) => s.orders().as_ref(),
        Stack::Hl7(analyte_config::RecordTransport::Folder(s)) => s.orders().as_ref(),
        Stack::Astm(analyte_config::RecordTransport::Stream(_))
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::Astm(analyte_config::RecordTransport::Folder(_)) => None,
    }
}

pub(super) fn law(machine: &Machine) -> Result<&LawStack, Error> {
    match machine.snapshot.effective().stack() {
        Stack::Law(s) => Ok(s),
        Stack::Astm(analyte_config::RecordTransport::Stream(_))
        | Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_)
        | Stack::Astm(analyte_config::RecordTransport::Folder(_))
        | Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => {
            Err(invalid("LAW settings absent"))
        }
    }
}

pub(super) fn send_ahead(machine: &Machine) -> bool {
    match machine.snapshot.effective().stack() {
        Stack::Astm(analyte_config::RecordTransport::Stream(s)) => {
            s.orders().as_ref().is_some_and(AstmOrders::send_ahead)
        }
        Stack::Astm(analyte_config::RecordTransport::Folder(s)) => {
            s.orders().as_ref().is_some_and(AstmOrders::send_ahead)
        }
        Stack::Hl7(analyte_config::RecordTransport::Stream(s)) => {
            s.orders().as_ref().is_some_and(|o| o.send().is_some())
        }
        Stack::Hl7(analyte_config::RecordTransport::Folder(s)) => {
            s.orders().as_ref().is_some_and(|o| o.send().is_some())
        }
        Stack::Law(s) => s
            .orders()
            .as_ref()
            .is_some_and(analyte_config::LawOrders::send_ahead),
        Stack::VidasLiteral(s) => s
            .orders()
            .as_ref()
            .is_some_and(analyte_config::VidasOrders::send_ahead),
        Stack::Autolyser(s) => s
            .orders()
            .as_ref()
            .is_some_and(analyte_config::AutolyserOrders::send_ahead),
        Stack::Poct(_) => false,
    }
}
