//! Validates outbound capabilities and code translations.
use super::*;
use crate::{ParsedMachine, StackChoice};
use std::collections::HashSet;

impl Orders {
    fn validate_stack(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
        if matches!(machine.choice(), StackChoice::LawManager) {
            return crate::schema::law::validate_orders(machine, path, self);
        }
        if !matches!(
            machine.choice(),
            StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager
        ) && (self.hl7.is_some() || self.hl7_send.is_some() || self.placer_namespace.is_some())
        {
            return Err(invalid(
                path,
                "orders.hl7",
                "HL7 query layout applies only to HL7",
            ));
        }
        let vendor_download = match machine.choice() {
            StackChoice::AstmLis02 | StackChoice::FileAstm => {
                if self.astm.is_none() {
                    return Err(invalid(
                        path,
                        "orders.astm",
                        "ASTM orders require explicit O-record destinations",
                    ));
                }
                None
            }
            StackChoice::VidasLiteral => Some(machine.vidas.as_ref().is_some_and(|v| v.download)),
            StackChoice::Autolyser => Some(machine.autolyser.as_ref().is_some_and(|a| a.download)),
            StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager => {
                self.validate_hl7(machine, path)?;
                return Ok(());
            }
            StackChoice::Poct => {
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
        if matches!(machine.choice(), StackChoice::Autolyser) {
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

    /// Refuses ambiguous tables, unbounded queries and invalid field ownership.
    pub(crate) fn validate(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
        self.validate_stack(machine, path)?;
        self.validate_test_tables(machine, path)?;
        self.validate_specimen_tables(machine, path)?;
        self.validate_installed_codes_and_priorities(machine, path)?;
        self.validate_astm_fields(machine, path)?;
        Ok(())
    }

    fn validate_test_tables(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
        if self.tests.is_empty()
            || self.tests.len() > 4096
            || self.priorities.is_empty()
            || self.priorities.len() > 3
            || self
                .query
                .as_ref()
                .is_some_and(|q| !(1..=60_000).contains(&q.deadline_ms))
        {
            return Err(invalid_tables(path));
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
                return Err(invalid_tables(path));
            }
        }
        if matches!(machine.choice(), StackChoice::VidasLiteral)
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
        Ok(())
    }

    fn validate_specimen_tables(
        &self,
        machine: &ParsedMachine<'_>,
        path: &str,
    ) -> Result<(), Error> {
        if let Some(types) = &self.specimen_types {
            let mut seen = HashSet::new();
            if types.is_empty()
                || types.len() > 4096
                || types.iter().any(|t| {
                    !seen.insert((&t.app.namespace, &t.app.value))
                        || match machine.choice() {
                            StackChoice::Autolyser => !matches!(t.machine.as_str(), "S" | "U"),
                            StackChoice::AstmLis02 | StackChoice::FileAstm => t
                                .machine
                                .as_str()
                                .bytes()
                                .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b)),
                            StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager => {
                                t.machine
                                    .as_str()
                                    .bytes()
                                    .any(|b| !(32..127).contains(&b) || b"|^~\\&".contains(&b))
                            }
                            StackChoice::VidasLiteral | StackChoice::Poct => true,
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
        Ok(())
    }

    fn validate_installed_codes_and_priorities(
        &self,
        machine: &ParsedMachine<'_>,
        path: &str,
    ) -> Result<(), Error> {
        if matches!(machine.choice(), StackChoice::Autolyser)
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
        if matches!(machine.choice(), StackChoice::Autolyser)
            && self.priorities.iter().any(|p| {
                !matches!(
                    p.machine,
                    MachinePriority::Code(PriorityCode::T | PriorityCode::R)
                )
            })
        {
            return Err(invalid(
                path,
                "orders.priorities",
                "Autolyser list codes are T (routine) and R (STAT)",
            ));
        }
        if matches!(
            machine.choice(),
            StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager
        ) && self.hl7_send.is_some()
            && self
                .priorities
                .iter()
                .any(|p| matches!(p.machine, MachinePriority::Custom(_)))
        {
            return Err(invalid(
                path,
                "orders.priorities",
                "HL7 send priorities must be R, S, A, P, C, T or PRN",
            ));
        }
        let mut priorities = HashSet::new();
        for priority in &self.priorities {
            if !matches!(priority.app.as_str(), "routine" | "urgent" | "stat")
                || !priorities.insert(&priority.app)
            {
                return Err(invalid_tables(path));
            }
        }
        Ok(())
    }

    fn validate_astm_fields(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
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
                return Err(invalid_tables(path));
            }
        }
        if astm
            .position
            .as_ref()
            .is_some_and(|f| !order_field(f, 4, machine))
        {
            return Err(invalid_tables(path));
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

fn order_field(field: &Field, number: u16, machine: &ParsedMachine<'_>) -> bool {
    matches!(field.group, Group::Order)
        && field.record.as_str() == "O"
        && field.field == number
        && usize::from(field.field) <= machine.syntax.fields
        && field.component > 0
        && usize::from(field.component) <= machine.syntax.components
        && field.repetition == 1
}

fn invalid_tables(path: &str) -> Error {
    invalid(
        path,
        "orders",
        "use bounded, unique outbound mappings and explicit protocol destinations",
    )
}
