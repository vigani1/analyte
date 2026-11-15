//! Explicit outbound settings, independent of result mappings.
use crate::{Error, Field, Group, Machine, SyntaxKind, invalid};
use analyte_model::{Identifier, NamespacedId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Unknown cache answers never become an empty work list.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenUntrusted {
    /// Return the protocol's query error.
    Error,
    /// Send no query response.
    NoAnswer,
}

/// Presence enables queries; both policy and deadline must be explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderQuery {
    /// Total query response budget in milliseconds.
    pub deadline_ms: u32,
    /// Policy for stale, unavailable or late lookups.
    pub when_untrusted: WhenUntrusted,
}

/// One app code maps to explicitly ordered ASTM universal test components.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundTest {
    /// Exact app code system and code.
    pub app: NamespacedId,
    /// Universal test ID components, including significant empty positions.
    pub machine_components: Vec<String>,
}

/// Explicit app specimen type to protocol sample-medium code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundSpecimen {
    /// Exact app code system and value.
    pub app: NamespacedId,
    /// Exact protocol code; ASTM uses explicit text, Autolyser supports S and U.
    pub machine: Identifier,
}

/// Priority translations are independent of inbound tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderPriority {
    /// App priority: routine, urgent or stat.
    pub app: Identifier,
    /// Exact machine priority.
    pub machine: Identifier,
}

/// ASTM O-record destinations; fields retain their standard ownership.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AstmOrderFields {
    /// Specimen ID destination.
    pub specimen: Field,
    /// Instrument specimen ID or position; presence requires supplied app data.
    pub position: Option<Field>,
    /// LIS02 section 8.4.16.1 specimen type: O field 16, component 1.
    pub specimen_type: Option<Field>,
    /// First component of each repeated universal test ID.
    pub test: Field,
    /// Priority destination.
    pub priority: Field,
    /// Action code destination.
    pub action: Field,
    /// Explicit new-order action code.
    pub action_code: Identifier,
}

/// Exact QRD filter agreed with this machine; unsupported filters fail closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7QueryFilter {
    /// QRD field number, excluding metadata, quantity and the specimen selector.
    pub field: u16,
    /// Exact wire value, including any explicitly agreed components.
    pub value: String,
}

/// One DSP-3 line, repeated in this order for each requested test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DsrLine {
    /// Exact specimen identifier.
    SampleId,
    /// Original app order identifier.
    OrderId,
    /// Explicit outbound test translation.
    TestCode,
    /// Explicit outbound priority translation.
    Priority,
    /// Supplied physical slot; absent stays empty.
    Position,
    /// Explicit specimen-type translation; absent stays empty.
    SpecimenType,
    /// Supplied patient ID under the configured namespace; absent stays empty.
    PatientId,
    /// Explicit presentation text, including empty padding lines.
    Literal {
        /// Exact plain display text.
        value: String,
    },
}

/// Legacy QRY/QCK/DSR interface agreement; no send-ahead is implied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7OrderQuery {
    /// QRD-8 or QRD-10 interpreted as the exact specimen ID, component 1 only.
    pub sample_field: u16,
    /// Exact trailing components for the specimen selector (empty only when agreed).
    pub sample_suffix: String,
    /// Every other nonempty QRD selector must match an explicit filter.
    pub filters: Vec<Hl7QueryFilter>,
    /// Ordered DSP lines for each test; at most 128 lines per test.
    pub lines: Vec<DsrLine>,
    /// Budget for the DSR application ACK, measured from wire issuance.
    pub acknowledgment_ms: u32,
}

/// Opt-in outbound order capability. Nothing is sent by its absence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orders {
    /// Optional query capability.
    pub query: Option<OrderQuery>,
    /// Explicit permission to send work lists ahead.
    pub send_ahead: bool,
    /// Outbound app-to-machine translations.
    pub tests: Vec<OutboundTest>,
    /// Optional explicit specimen-type translations; absence maps no values.
    pub specimen_types: Option<Vec<OutboundSpecimen>>,
    /// Explicit priority translations.
    pub priorities: Vec<OrderPriority>,
    /// Accepted assigning system for the unqualified vendor patient field.
    /// Required for VIDAS and Autolyser outbound orders; never inferred.
    pub patient_namespace: Option<Identifier>,
    /// Explicit Autolyser clone behavior, including an explicit false.
    pub clone: Option<bool>,
    /// ASTM record layout, required only by the ASTM stack.
    pub astm: Option<AstmOrderFields>,
    /// Explicit legacy HL7 query layout and acknowledgment deadline.
    pub hl7: Option<Hl7OrderQuery>,
}

impl Orders {
    fn validate_stack(&self, machine: &Machine, path: &str) -> Result<(), Error> {
        if machine.syntax.kind != SyntaxKind::Hl7V2 && self.hl7.is_some() {
            return Err(invalid(
                path,
                "orders.hl7",
                "HL7 query layout applies only to HL7",
            ));
        }
        let vendor_download = match machine.syntax.kind {
            SyntaxKind::AstmLis02 => {
                if self.astm.is_none() {
                    return Err(invalid(
                        path,
                        "orders.astm",
                        "ASTM orders require explicit O-record destinations",
                    ));
                }
                None
            }
            SyntaxKind::VidasLiteral => Some(machine.vidas.as_ref().is_some_and(|v| v.download)),
            SyntaxKind::Autolyser => Some(machine.autolyser.as_ref().is_some_and(|a| a.download)),
            SyntaxKind::Hl7V2 => {
                self.validate_hl7(machine, path)?;
                return Ok(());
            }
            SyntaxKind::Poct => {
                return Err(invalid(
                    path,
                    "orders",
                    "outbound orders are unsupported for this stack",
                ));
            }
        };
        if let Some(download) = vendor_download {
            if self.patient_namespace.is_none() {
                return Err(invalid(
                    path,
                    "orders.patient_namespace",
                    "supply the explicit assigning system for the vendor patient field",
                ));
            }
            if self.astm.is_some() {
                return Err(invalid(
                    path,
                    "orders.astm",
                    "ASTM destinations do not apply to vendor orders",
                ));
            }
            if self.query.is_some() {
                return Err(invalid(
                    path,
                    "orders.query",
                    "vendor work lists do not support ASTM queries",
                ));
            }
            if self.send_ahead && !download {
                return Err(invalid(
                    path,
                    "orders.send_ahead",
                    "vendor send-ahead also requires explicit download permission",
                ));
            }
        } else if self.patient_namespace.is_some() {
            return Err(invalid(
                path,
                "orders.patient_namespace",
                "this outbound layout has no patient field",
            ));
        }
        if machine.syntax.kind == SyntaxKind::Autolyser {
            if self.clone.is_none() {
                return Err(invalid(
                    path,
                    "orders.clone",
                    "supply explicit Autolyser clone behavior (true or false)",
                ));
            }
        } else if self.clone.is_some() {
            return Err(invalid(
                path,
                "orders.clone",
                "clone behavior applies only to Autolyser orders",
            ));
        }
        Ok(())
    }

    fn validate_hl7(&self, machine: &Machine, path: &str) -> Result<(), Error> {
        let layout = self.hl7.as_ref().ok_or_else(|| {
            invalid(
                path,
                "orders.hl7",
                "supply explicit HL7 query selectors, DSP layout and ACK deadline",
            )
        })?;
        if self.send_ahead || self.query.is_none() || self.astm.is_some() || self.clone.is_some() {
            return Err(invalid(
                path,
                "orders",
                "HL7 currently supports explicit legacy queries only",
            ));
        }
        if self
            .query
            .as_ref()
            .is_some_and(|query| !matches!(query.when_untrusted, WhenUntrusted::Error))
        {
            return Err(invalid(
                path,
                "orders.query.when_untrusted",
                "HL7 unavailable queries require an explicit AE response (error)",
            ));
        }
        if !matches!(layout.sample_field, 8 | 10) {
            return Err(invalid(
                path,
                "orders.hl7.sample_field",
                "choose QRD-8 or QRD-10 for the agreed specimen selector",
            ));
        }
        if layout.sample_suffix.len() > 250
            || !layout.sample_suffix.is_ascii()
            || (!layout.sample_suffix.is_empty() && !layout.sample_suffix.starts_with('^'))
            || layout
                .sample_suffix
                .bytes()
                .any(|b| b.is_ascii_control() || b"|~\\".contains(&b))
        {
            return Err(invalid(
                path,
                "orders.hl7.sample_suffix",
                "supply exact trailing selector components, beginning with ^, or an explicit empty string",
            ));
        }
        let mut seen = HashSet::new();
        if layout.filters.len() > 6
            || layout.filters.iter().any(|f| {
                !(5..=12).contains(&f.field)
                    || matches!(f.field, 7)
                    || f.field == layout.sample_field
                    || !seen.insert(f.field)
                    || f.value.is_empty()
                    || f.value.len() > 250
                    || !f.value.is_ascii()
                    || f.value.bytes().any(|b| b.is_ascii_control() || b == b'|')
            })
            || [8, 9, 10]
                .into_iter()
                .any(|field| field != layout.sample_field && !seen.contains(&field))
        {
            return Err(invalid(
                path,
                "orders.hl7.filters",
                "provide unique exact selectors including all required QRD fields other than the sample",
            ));
        }
        let mut fields = vec![String::new(); 12];
        for (number, value) in [
            (1_usize, "20000101"),
            (2, "D"),
            (3, "D"),
            (4, "config"),
            (7, "1^LI"),
        ] {
            if let Some(field) = number
                .checked_sub(1)
                .and_then(|index| fields.get_mut(index))
            {
                *field = value.to_owned();
            }
        }
        if let Some(field) = usize::from(layout.sample_field)
            .checked_sub(1)
            .and_then(|index| fields.get_mut(index))
        {
            *field = format!("CONFIG{}", layout.sample_suffix);
        }
        for filter in &layout.filters {
            if let Some(field) = usize::from(filter.field)
                .checked_sub(1)
                .and_then(|index| fields.get_mut(index))
            {
                *field = filter.value.clone();
            }
        }
        let edition = match machine.syntax.edition {
            Some(crate::Edition::V231) => "2.3.1",
            Some(crate::Edition::V251) => "2.5.1",
            None => return Err(invalid(path, "syntax.edition", "HL7 requires an edition")),
        };
        let wire = format!(
            "MSH|^~\\&|HOST|LAB|PEER|LAB|20000101||QRY^Q02|config|P|{edition}\rQRD|{}\r",
            fields.join("|")
        );
        let syntax = analyte_hl7::Message::parse(wire.as_bytes(), analyte_hl7::Options::default())
            .and_then(|message| analyte_hl7::query::Request::new(&message).map(|_| ()));
        if syntax.is_err() {
            return Err(invalid(
                path,
                "orders.hl7.filters",
                "selector suffix and filters must form a valid edition-specific QRD",
            ));
        }
        if !(1..=60_000).contains(&layout.acknowledgment_ms) {
            return Err(invalid(
                path,
                "orders.hl7.acknowledgment_ms",
                "use 1 to 60000 milliseconds",
            ));
        }
        if layout.lines.is_empty() || layout.lines.len()>128 || !layout.lines.iter().any(|l|matches!(l,DsrLine::SampleId)) || !layout.lines.iter().any(|l|matches!(l,DsrLine::TestCode)) || layout.lines.iter().any(|line| matches!(line,DsrLine::Literal{value} if value.len()>300 || !value.is_ascii() || value.bytes().any(|b|b.is_ascii_control() || b"|^~\\&".contains(&b)))) {
            return Err(invalid(path,"orders.hl7.lines","supply 1 to 128 plain DSP lines including sample_id and test_code"));
        }
        if layout.lines.iter().any(|l| matches!(l, DsrLine::PatientId))
            != self.patient_namespace.is_some()
        {
            return Err(invalid(
                path,
                "orders.patient_namespace",
                "supply a namespace exactly when patient_id is mapped",
            ));
        }
        if self.tests.iter().any(|t| t.machine_components.len() != 1) {
            return Err(invalid(
                path,
                "orders.tests",
                "HL7 display tests require one explicit plain text code",
            ));
        }
        Ok(())
    }

    /// Refuses ambiguous tables, unbounded queries and invalid field ownership.
    pub(crate) fn validate(&self, machine: &Machine, path: &str) -> Result<(), Error> {
        let fail = || {
            invalid(
                path,
                "orders",
                "use bounded, unique outbound mappings and explicit protocol destinations",
            )
        };
        self.validate_stack(machine, path)?;
        if self.tests.is_empty()
            || self.tests.len() > 4096
            || self.priorities.is_empty()
            || self.priorities.len() > 3
            || self
                .query
                .as_ref()
                .is_some_and(|q| !(1..=60_000).contains(&q.deadline_ms))
        {
            return Err(fail());
        }
        let mut tests = HashSet::new();
        for test in &self.tests {
            if !tests.insert((&test.app.namespace, &test.app.value))
                || test.machine_components.is_empty()
                || test.machine_components.len() > machine.syntax.components
                || test.machine_components.iter().any(|s| {
                    s.len() > 256 || !s.is_ascii() || s.bytes().any(|b| b < 32 || b == 127)
                })
                || test.machine_components.iter().all(String::is_empty)
            {
                return Err(fail());
            }
        }
        if machine.syntax.kind == SyntaxKind::VidasLiteral
            && self.tests.iter().any(|test| {
                let [code] = test.machine_components.as_slice() else {
                    return true;
                };
                code.is_empty()
                    || code.len() > 4
                    || code.bytes().any(|b| !(32..=126).contains(&b) || b == b'|')
            })
        {
            return Err(invalid(
                path,
                "orders.tests",
                "VIDAS requires one printable assay code of 1 to 4 bytes without a pipe",
            ));
        }
        if let Some(types) = &self.specimen_types {
            let mut seen = HashSet::new();
            if types.is_empty()
                || types.len() > 4096
                || types.iter().any(|t| {
                    !seen.insert((&t.app.namespace, &t.app.value))
                        || match machine.syntax.kind {
                            SyntaxKind::Autolyser => !matches!(t.machine.as_str(), "S" | "U"),
                            SyntaxKind::AstmLis02 => t
                                .machine
                                .as_str()
                                .bytes()
                                .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b)),
                            SyntaxKind::Hl7V2 => t
                                .machine
                                .as_str()
                                .bytes()
                                .any(|b| !(32..127).contains(&b) || b"|^~\\&".contains(&b)),
                            SyntaxKind::VidasLiteral | SyntaxKind::Poct => true,
                        }
                })
            {
                return Err(invalid(
                    path,
                    "orders.specimen_types",
                    "use unique explicit specimen mappings valid for the selected protocol",
                ));
            }
        }
        if machine.syntax.kind == SyntaxKind::Autolyser
            && self.tests.iter().any(|test| {
                let [code] = test.machine_components.as_slice() else {
                    return true;
                };
                code.len() != 4 || !code.bytes().all(|b| (32..=126).contains(&b))
            })
        {
            return Err(invalid(
                path,
                "orders.tests",
                "Autolyser requires one exact four-byte printable analysis code",
            ));
        }
        if machine.syntax.kind == SyntaxKind::Autolyser
            && self
                .priorities
                .iter()
                .any(|p| !matches!(p.machine.as_str(), "T" | "R"))
        {
            return Err(invalid(
                path,
                "orders.priorities",
                "Autolyser list codes are T (routine) and R (STAT)",
            ));
        }
        let mut priorities = HashSet::new();
        for priority in &self.priorities {
            if !matches!(priority.app.as_str(), "routine" | "urgent" | "stat")
                || !priorities.insert(&priority.app)
            {
                return Err(fail());
            }
        }
        let Some(astm) = &self.astm else {
            return Ok(());
        };
        for (field, number) in [
            (&astm.specimen, 3),
            (&astm.test, 5),
            (&astm.priority, 6),
            (&astm.action, 12),
        ] {
            if !order_field(field, number, machine) {
                return Err(fail());
            }
        }
        if astm
            .position
            .as_ref()
            .is_some_and(|f| !order_field(f, 4, machine))
        {
            return Err(fail());
        }
        if astm
            .specimen_type
            .as_ref()
            .is_some_and(|f| !order_field(f, 16, machine) || f.component != 1)
        {
            return Err(invalid(
                path,
                "orders.astm.specimen_type",
                "LIS02 specimen type uses O field 16, component 1 only",
            ));
        }
        Ok(())
    }
}

fn order_field(field: &Field, number: u16, machine: &Machine) -> bool {
    matches!(field.group, Group::Order)
        && field.record.as_str() == "O"
        && field.field == number
        && usize::from(field.field) <= machine.syntax.fields
        && field.component > 0
        && usize::from(field.component) <= machine.syntax.components
        && field.repetition == 1
}
