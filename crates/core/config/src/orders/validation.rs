//! Validates outbound capabilities, priority translations and the specimen code system.
use super::{DsrLine, Error, Field, Group, MachinePriority, Orders, PriorityCode, invalid};
use crate::ParsedMachine;
use std::collections::HashSet;

impl Orders {
    fn validate_stack(&self, machine: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
        if self.vidas_patient_namespace_from_site
            && (!machine.choice().has_vidas_assay_codes() || self.patient_namespace.is_some())
        {
            return Err(invalid(
                path,
                "orders.vidas_patient_namespace_from_site",
                "only VIDAS orders without a fixed patient namespace can use the site namespace",
            ));
        }
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
            if self.patient_namespace.is_none() && !self.vidas_patient_namespace_from_site {
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
        self.validate_specimen_system(machine, path)?;
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

    /// ADR 0030: the machine's specimen code system is declared where its
    /// protocol writes a specimen type, and nowhere else. The code's own format
    /// limits are checked when an order is mapped.
    fn validate_specimen_system(
        &self,
        machine: &ParsedMachine<'_>,
        path: &str,
    ) -> Result<(), Error> {
        let choice = machine.choice();
        // (required, allowed, reason)
        let (required, allowed, reason) = if choice.allows_law_section() {
            // LAW SPM-4 is required (TF-2b); law::validate_orders checks it.
            return Ok(());
        } else if choice.has_astm_records() {
            let destination = self
                .astm
                .as_ref()
                .is_some_and(|a| a.specimen_type.is_some());
            (
                destination,
                destination,
                "declare specimen_system exactly when [orders.astm] has a specimen_type destination",
            )
        } else if choice.has_autolyser_orders() {
            (
                true,
                true,
                "Autolyser work lists name a sample medium; declare the machine's specimen_system",
            )
        } else if choice.has_vidas_assay_codes() {
            (false, false, "VIDAS Literal has no specimen-type tag")
        } else if choice.has_hl7_records() {
            // ORM may leave the specimen out; CWE editions require it, which
            // validate_hl7_send_codes checks.
            let displayed = self
                .hl7
                .as_ref()
                .is_some_and(|q| q.lines.iter().any(|l| matches!(l, DsrLine::SpecimenType)));
            (
                displayed,
                displayed || self.hl7_send.is_some(),
                "HL7 writes a specimen type only in send-ahead orders or a specimen_type display line",
            )
        } else {
            return Ok(());
        };
        let declared = self.specimen_system.is_some();
        if (declared && !allowed) || (required && !declared) {
            return Err(invalid(path, "orders.specimen_system", reason));
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
        if astm.batch_container.is_some()
            != (self.send_ahead && astm.profile == Some(super::AstmOrderProfile::Cobas))
            || astm
                .batch_container
                .as_ref()
                .is_some_and(|v| !matches!(v.as_str(), "SC" | "MC"))
        {
            return Err(invalid(
                path,
                "orders.astm.batch_container",
                "cobas batch downloads require an explicit SC or MC container",
            ));
        }
        if let Some(profile) = astm.profile {
            use super::AstmOrderProfile::{Cobas, Elecsys};
            let (action, specimen) = match profile {
                Elecsys => ("N", false),
                Cobas => ("A", true),
            };
            if astm.position.is_some()
                || astm.action_code.as_str() != action
                || astm.specimen_type.is_some() != specimen
                || [
                    astm.specimen.component,
                    astm.priority.component,
                    astm.action.component,
                ] != [1; 3]
                || astm.test.component != 4
                || astm.test_components.is_some_and(|n| n != 5)
                || machine.syntax.components < 6
                || machine.syntax.fields < 26
                || self
                    .query
                    .as_ref()
                    .is_some_and(|q| matches!(q.when_untrusted, super::WhenUntrusted::NoAnswer))
                || self
                    .priorities
                    .iter()
                    .any(|p| !matches!(p.machine.as_str(), "R" | "S"))
                || (self.send_ahead && profile == Elecsys)
                || astm.receiver.as_ref().is_some_and(|id| {
                    id.as_str().len() > 30
                        || !id
                            .as_str()
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(invalid(
                    path,
                    "orders.astm.profile",
                    "incompatible structured ASTM order layout",
                ));
            }
        }
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
