//! Validates outbound capabilities, priority and specimen translations.
use super::{Error, Field, Group, MachinePriority, Orders, PriorityCode, invalid};
use crate::ParsedMachine;
use std::collections::HashSet;

impl Orders {
    fn validate_stack(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
        if machine.choice().allows_law_section() {
            return crate::schema::law::validate_orders(machine, path, self);
        }
        if !machine.choice().has_hl7_records()
            && (self.hl7.is_some() || self.hl7_send.is_some() || self.placer_namespace.is_some())
        {
            return Err(invalid(
                path,
                "orders.hl7",
                "HL7 query layout applies only to HL7",
            ));
        }
        let vendor_download = if machine.choice().has_astm_records() {
            if self.astm.is_none() {
                return Err(invalid(
                    path,
                    "orders.astm",
                    "ASTM orders require explicit O-record destinations",
                ));
            }
            None
        } else if machine.choice().has_vidas_assay_codes() {
            Some(machine.vidas.as_ref().is_some_and(|v| v.download))
        } else if machine.choice().has_autolyser_orders() {
            Some(machine.autolyser.as_ref().is_some_and(|a| a.download))
        } else if machine.choice().has_hl7_records() {
            self.validate_hl7(machine, path)?;
            return Ok(());
        } else {
            return Err(invalid(
                path,
                "orders",
                "outbound orders are unsupported for this stack",
            ));
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
        if machine.choice().has_autolyser_orders() {
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
        self.validate_query_and_priority_bounds(path)?;
        self.validate_specimen_tables(machine, path)?;
        self.validate_installed_codes_and_priorities(machine, path)?;
        self.validate_astm_fields(machine, path)?;
        Ok(())
    }

    fn validate_query_and_priority_bounds(&self, path: &str) -> Result<(), Error> {
        if self.priorities.is_empty()
            || self.priorities.len() > 3
            || self
                .query
                .as_ref()
                .is_some_and(|q| !(1..=60_000).contains(&q.deadline_ms))
        {
            return Err(invalid_tables(path));
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
                        || match machine.choice().record_kind() {
                            crate::stack::RecordKind::Astm => t
                                .machine
                                .as_str()
                                .bytes()
                                .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b)),
                            crate::stack::RecordKind::Hl7 => t
                                .machine
                                .as_str()
                                .bytes()
                                .any(|b| !(32..127).contains(&b) || b"|^~\\&".contains(&b)),
                            crate::stack::RecordKind::Vendor => {
                                !machine.choice().has_autolyser_orders()
                                    || !matches!(t.machine.as_str(), "S" | "U")
                            }
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
        if machine.choice().has_autolyser_orders()
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
        if machine.choice().has_hl7_records()
            && self.hl7_send.is_some()
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
        if astm.test_components.is_some_and(|count| {
            count < astm.test.component || usize::from(count) > machine.syntax.components
        }) {
            return Err(invalid(
                path,
                "orders.astm.test_components",
                "count at least the code's component and at most the syntax component limit",
            ));
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
