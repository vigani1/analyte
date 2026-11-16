//! Validates configured HL7 outbound message layouts.
use super::*;
use crate::Edition;
use std::collections::HashSet;

impl Orders {
    pub(super) fn validate_hl7(&self, machine: &MachineDocument, path: &str) -> Result<(), Error> {
        if (self.query.is_some() || self.hl7.is_some())
            && let Some(edition) = machine.syntax.edition.map(analyte_hl7::Version::from)
            && let Some(section) = edition.legacy_query_withdrawal()
        {
            return Err(Error::WithdrawnHl7Query {
                path: path.into(),
                edition: edition.as_str(),
                section,
            });
        }
        if self.astm.is_some()
            || self.clone.is_some()
            || self.send_ahead != self.hl7_send.is_some()
            || self.query.is_some() != self.hl7.is_some()
        {
            return Err(invalid(
                path,
                "orders",
                "declare each HL7 capability and its own layout explicitly",
            ));
        }
        self.validate_hl7_send(machine, path)?;
        if self.query.is_none() {
            return Ok(());
        }
        let layout = self.hl7.as_ref().ok_or_else(|| {
            invalid(
                path,
                "orders.hl7",
                "supply explicit HL7 query selectors, DSP layout and ACK deadline",
            )
        })?;
        if self.query.is_none() || self.astm.is_some() || self.clone.is_some() {
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
        self.validate_query_selectors(layout, path)?;
        self.validate_query_syntax(machine, layout, path)?;
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
            && self.patient_namespace.is_none()
        {
            return Err(invalid(
                path,
                "orders.patient_namespace",
                "supply a namespace exactly when patient_id is mapped",
            ));
        }
        if self
            .tests
            .iter()
            .any(|t| t.machine_components.len() != if self.send_ahead { 3 } else { 1 })
        {
            return Err(invalid(
                path,
                "orders.tests",
                "HL7 display tests require one explicit plain text code",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_hl7_send(
        &self,
        machine: &MachineDocument,
        path: &str,
    ) -> Result<(), Error> {
        if let Some(send) = &self.hl7_send {
            if let Some(reason) = machine
                .syntax
                .edition
                .map(analyte_hl7::Version::from)
                .and_then(analyte_hl7::Version::order_sending_blocker)
            {
                return Err(invalid(path, "orders.hl7_send", reason));
            }
            let plain = |value: &Identifier| {
                value.as_str().is_ascii()
                    && value
                        .as_str()
                        .bytes()
                        .all(|b| !b.is_ascii_control() && !b"|^~\\&".contains(&b))
            };
            if self
                .placer_namespace
                .as_ref()
                .is_none_or(|namespace| !plain(namespace) || namespace.as_str().len() > 18)
            {
                return Err(invalid(
                    path,
                    "orders.placer_namespace",
                    "HL7 send-ahead requires a plain EI namespace fitting the 22-byte identity",
                ));
            }
            if ![&send.application, &send.peer_application]
                .into_iter()
                .all(|value| plain(value) && value.as_str().len() <= 20)
                || [&send.facility, &send.peer_facility]
                    .into_iter()
                    .flatten()
                    .any(|value| !plain(value) || value.as_str().len() > 20)
                || !(1..=60_000).contains(&send.response_ms)
            {
                return Err(invalid(
                    path,
                    "orders.hl7_send",
                    "supply explicit bounded headers, processing and response deadline",
                ));
            }
            if !matches!(send.sample_field, 18 | 19)
                || send.position_field.is_some_and(|field| {
                    !matches!(field, 18 | 19)
                        || field == send.sample_field
                        || machine.syntax.edition != Some(Edition::V231)
                })
            {
                return Err(invalid(
                    path,
                    "orders.hl7_send.position_field",
                    "ORM position must use the other placer field; OML position is not supported",
                ));
            }
            self.validate_hl7_send_codes(machine, send, path)?;
        } else if self.placer_namespace.is_some() {
            return Err(invalid(
                path,
                "orders.placer_namespace",
                "a placer namespace belongs to HL7 send-ahead",
            ));
        }
        Ok(())
    }
    fn validate_hl7_send_codes(
        &self,
        machine: &MachineDocument,
        send: &Hl7Send,
        path: &str,
    ) -> Result<(), Error> {
        let plain = |value: &Identifier| {
            analyte_hl7::is_safe_field(
                value.as_str().as_bytes(),
                20,
                analyte_hl7::FieldSyntax::Plain,
            )
        };
        let modern = machine
            .syntax
            .edition
            .map(analyte_hl7::Version::from)
            .is_some_and(analyte_hl7::Version::uses_dtm);
        if modern
            && (send
                .priority_version
                .as_ref()
                .is_none_or(|v| !plain(v) || v.as_str().len() > 10)
                || send
                    .specimen_version
                    .as_ref()
                    .is_none_or(|v| !plain(v) || v.as_str().len() > 10)
                || send.specimen_system.is_none())
        {
            return Err(invalid(
                path,
                "orders.hl7_send",
                "supply explicit specimen and priority coding-system revisions",
            ));
        }
        if self.tests.iter().any(|test| {
            test.machine_components.len() != if modern { 7 } else { 3 }
                || (modern
                    && (test
                        .machine_components
                        .get(3..6)
                        .is_none_or(|v| v.iter().any(|v| !v.is_empty()))
                        || test
                            .machine_components
                            .get(6)
                            .is_none_or(|v| v.is_empty() || v.len() > 10)))
                || test.machine_components.first().is_none_or(String::is_empty)
                || test.machine_components.get(2).is_none_or(String::is_empty)
        }) {
            return Err(invalid(
                path,
                "orders.tests",
                "HL7 orders require explicit code, text and coding system components",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_query_syntax(
        &self,
        machine: &MachineDocument,
        layout: &Hl7OrderQuery,
        path: &str,
    ) -> Result<(), Error> {
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
        let edition = analyte_hl7::Version::from(
            machine
                .syntax
                .edition
                .ok_or_else(|| invalid(path, "syntax.edition", "HL7 requires an edition"))?,
        )
        .as_str();
        let wire = format!(
            "MSH|^~\\&|HOST|LAB|PEER|LAB|20000101||QRY^Q02^QRY_Q02|config|P|{edition}\rQRD|{}\r",
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
        Ok(())
    }
    pub(super) fn validate_query_selectors(
        &self,
        layout: &Hl7OrderQuery,
        path: &str,
    ) -> Result<(), Error> {
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
        Ok(())
    }
}
