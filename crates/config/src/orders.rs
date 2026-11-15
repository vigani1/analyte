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
    /// First component of each repeated universal test ID.
    pub test: Field,
    /// Priority destination.
    pub priority: Field,
    /// Action code destination.
    pub action: Field,
    /// Explicit new-order action code.
    pub action_code: Identifier,
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
    /// Explicit priority translations.
    pub priorities: Vec<OrderPriority>,
    /// ASTM record layout.
    pub astm: AstmOrderFields,
}

impl Orders {
    pub(crate) fn validate(&self, machine: &Machine, path: &str) -> Result<(), Error> {
        let fail = || {
            invalid(
                path,
                "orders",
                "use bounded, unique outbound mappings and standard O-record destinations",
            )
        };
        if machine.syntax.kind != SyntaxKind::AstmLis02
            || self.tests.is_empty()
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
        let mut priorities = HashSet::new();
        for priority in &self.priorities {
            if !matches!(priority.app.as_str(), "routine" | "urgent" | "stat")
                || !priorities.insert(&priority.app)
            {
                return Err(fail());
            }
        }
        for (field, number) in [
            (&self.astm.specimen, 3),
            (&self.astm.test, 5),
            (&self.astm.priority, 6),
            (&self.astm.action, 12),
        ] {
            if !order_field(field, number, machine) {
                return Err(fail());
            }
        }
        if self
            .astm
            .position
            .as_ref()
            .is_some_and(|f| !order_field(f, 4, machine))
        {
            return Err(fail());
        }
        Ok(())
    }
}

fn order_field(field: &Field, number: u16, machine: &Machine) -> bool {
    matches!(field.group, Group::Order)
        && field.record.as_str() == "O"
        && field.field == number
        && field.component > 0
        && usize::from(field.component) <= machine.syntax.components
        && field.repetition == 1
}
