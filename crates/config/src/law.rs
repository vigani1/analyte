//! Explicit LAW profile selection; TF-1 section 5.2 and TF-2b sections 2.2/3.27.
use crate::{DialogKind, Edition, Encoding, Error, Escapes, Machine, Terminator, invalid};
use serde::{Deserialize, Serialize};

/// Supported host lookup scopes. Other LAW scopes must receive an error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LawQueryForm {
    /// Exact specimen/container identifier using the configured sample identity.
    Container,
    /// Pending work addressed to this analyzer, under the shared cache trust rule.
    AllPending,
}

/// Explicit peer support, with no implicit enhanced-content permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LawPeer {
    /// Peer accepts patient demographics.
    pub patient: bool,
    /// Peer accepts container characteristics.
    pub container: bool,
    /// Peer accepts specimen characteristics.
    pub specimen: bool,
    /// Peer accepts explicit dilution instructions.
    pub dilution: bool,
    /// Peer accepts app-authorized manager repeats.
    pub manager_repeat: bool,
    /// Peer accepts app-supplied repeat/reflex permissions.
    pub repeat_control: bool,
    /// Peer accepts explicitly mapped priority.
    pub priority: bool,
}

impl LawPeer {
    /// Supplies the existing LAW builders' peer-capability intersection.
    #[must_use]
    pub fn capabilities(&self) -> analyte_hl7::law::Capabilities {
        analyte_hl7::law::Capabilities {
            patient: self.patient,
            container: self.container,
            specimen: self.specimen,
            dilution: self.dilution,
            manager_repeat: self.manager_repeat,
            repeat_control: self.repeat_control,
            priority: self.priority,
        }
    }
}

/// Analyzer Manager policy, separate from generic HL7 and legacy display queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Law {
    /// Explicit supported lookup scopes; no positional or parent-ID inference.
    pub query_forms: Vec<LawQueryForm>,
    /// Peer-declared enhanced-content support, consumed by existing LAW builders.
    pub peer: LawPeer,
    /// First query byte through storage, lookup and reservation.
    pub query_deadline_ms: u32,
    /// LAB-28 application response deadline after issuance.
    pub response_ms: u32,
}

pub(crate) fn validate(machine: &Machine, path: &str) -> Result<(), Error> {
    if (machine.dialog.kind == DialogKind::LawManager) != machine.law.is_some() {
        return Err(invalid(
            path,
            "law",
            "supply LAW settings only for the law_manager dialog",
        ));
    }
    let Some(law) = &machine.law else {
        return Ok(());
    };
    if machine.syntax.edition != Some(Edition::V251)
        || machine.syntax.encoding != Encoding::Utf8
        || machine.syntax.terminator != Some(Terminator::Cr)
        || machine.syntax.escapes != Some(Escapes::Reject)
    {
        return Err(invalid(
            path,
            "syntax",
            "LAW requires HL7 2.5.1, UTF-8, CR segments and strict escapes",
        ));
    }
    if law.query_forms.is_empty()
        || law.query_forms.len() > 2
        || (law.query_forms.len() == 2 && law.query_forms.first() == law.query_forms.get(1))
    {
        return Err(invalid(
            path,
            "law.query_forms",
            "declare one or both supported query forms once",
        ));
    }
    if !(1..=60_000).contains(&law.query_deadline_ms) || !(1..=60_000).contains(&law.response_ms) {
        return Err(invalid(
            path,
            "law",
            "use query and response deadlines from 1 through 60000 milliseconds",
        ));
    }
    if machine.mappings.as_ref().is_some_and(|mapping| {
        mapping.identity.correction_status_codes.len() != 1
            || mapping
                .identity
                .correction_status_codes
                .first()
                .is_none_or(|code| code.as_str() != "C")
    }) {
        return Err(invalid(
            path,
            "mappings.identity.correction_status_codes",
            "LAW corrections require exactly the explicit status C",
        ));
    }
    if machine.mappings.as_ref().is_some_and(|mapping| {
        let selects = |field: &crate::Field, group, record, number, component| {
            field.group == group
                && field.record.as_str() == record
                && field.field == number
                && field.component == component
                && field.repetition == 1
        };
        !selects(&mapping.identity.order_id, crate::Group::Order, "OBR", 2, 1)
            || match &mapping.identity.order_namespace {
                crate::OrderNamespace::Fixed { .. } => false,
                crate::OrderNamespace::Field { field } => {
                    !selects(field, crate::Group::Order, "OBR", 2, 2)
                }
            }
            || match &mapping.identity.sub_id {
                crate::SubId::Field(field) => {
                    !selects(field, crate::Group::Observation, "OBX", 4, 1)
                }
                crate::SubId::None(_) => true,
            }
            || !mapping.identity.run.is_empty()
    }) {
        return Err(invalid(
            path,
            "mappings.identity",
            "LAW uses OBR-2 AWOS and complete OBX-4 OG; use OBR-2.2 or an explicit fixed AWOS namespace and no extra run selectors",
        ));
    }
    if machine
        .orders
        .as_ref()
        .is_some_and(|orders| orders.hl7.is_some() || orders.hl7_send.is_some())
    {
        return Err(invalid(
            path,
            "orders",
            "LAW cannot use legacy queries or non-LAW work messages",
        ));
    }
    Ok(())
}
