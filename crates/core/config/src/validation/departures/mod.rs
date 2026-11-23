//! Checks named departures against installed applicability and selected settings.
use crate::ParsedMachine;
use crate::{
    ChecksumHex, DepartureDirection, DepartureEdition, DepartureGuarantee, DepartureMessage,
    DepartureReference, DepartureScope, Encoding, Error, Escapes, EvidenceLevel, MachineDeparture,
    Priority, Terminator,
};

use crate::DepartureRefusal as Refusal;

mod evidence;

fn refusal(path: &str, rule: &str, reason: Refusal) -> Error {
    Error::Departure {
        path: path.into(),
        rule: rule.into(),
        reason,
        source: None,
    }
}

pub(super) fn validate_departures(
    m: &ParsedMachine<'_>,
    path: &str,
    reference_path: &str,
) -> Result<(), Error> {
    if m.deviations.len() > 12 {
        return Err(refusal(path, "deviations", Refusal::TooManyDeclarations));
    }
    let mut tags = std::collections::HashSet::new();
    for departure in &m.deviations {
        departure.evidence_snapshot()?;
        let tag = departure.tag();
        if !tags.insert(tag) {
            return Err(refusal(path, tag, Refusal::DuplicateRule));
        }
        if tags.contains("hl7_cr_lf_terminators") && tags.contains("hl7_lf_terminators") {
            return Err(refusal(path, tag, Refusal::ConflictingVariants));
        }
    }
    for departure in &m.deviations {
        let tag = departure.tag();
        validate_scope(m, departure).map_err(|r| refusal(path, tag, r))?;
        let exercised = |d: &MachineDeparture, file: &analyte_model::Identifier| {
            evidence::exercised_capability(m, d, file)
        };
        evidence::validate(departure, reference_path, &exercised).map_err(|failure| {
            Error::Departure {
                path: path.into(),
                rule: tag.into(),
                reason: failure.reason,
                source: failure.source,
            }
        })?;
    }
    for (tag, selected) in selected_settings(m) {
        if selected && !tags.contains(tag) {
            return Err(refusal(path, tag, Refusal::MissingDeclaration));
        }
    }
    Ok(())
}

fn selected_settings(m: &ParsedMachine<'_>) -> [(&'static str, bool); 9] {
    let s = &m.syntax;
    let astm = m.choice().has_astm_link();
    [
        ("hl7_utf8_in_v231", s.allow_utf8_in_v231),
        (
            "hl7_cr_lf_terminators",
            s.terminator == Some(Terminator::CrLf),
        ),
        ("hl7_lf_terminators", s.terminator == Some(Terminator::Lf)),
        (
            "hl7_preserve_unsupported_escapes",
            s.escapes == Some(Escapes::Preserve),
        ),
        ("astm_frame_limit", astm && m.framing.limit != 64000),
        (
            "astm_receive_timeout",
            astm && m.dialog.receive_timeout_ms != 30000,
        ),
        (
            "astm_send_timeout",
            astm && m.dialog.send_timeout_ms != 15000,
        ),
        (
            "astm_host_contention_priority",
            m.dialog.contention_priority == Some(Priority::Host),
        ),
        (
            "astm_lowercase_checksum_hex",
            m.framing.checksum_hex == Some(ChecksumHex::AcceptLowercase),
        ),
    ]
}

/// Refuses a wildcard or unspecified model, mode or firmware; only firmware
/// may be `unknown`, and then only with explicit vendor coverage.
fn applicability(scope: &DepartureScope) -> Result<(), Refusal> {
    for text in [&scope.model, &scope.mode, &scope.firmware] {
        if text.as_str().contains(['*', '?']) || matches!(text.as_str(), "all" | "any" | "untested")
        {
            return Err(Refusal::UnsupportedApplicability);
        }
    }
    if scope.model.as_str() == "unknown" || scope.mode.as_str() == "unknown" {
        return Err(Refusal::UnsupportedApplicability);
    }
    Ok(())
}

/// Checks the departures a model's `record.toml` declares for a record-only
/// connection (ADR 0034 with ADR 0015). Only packed ASTM records in one
/// verified inbound frame can be declared: every other link rule is the
/// shipped capture profile's. Each declaration names this model by its folder
/// key (`model`, `vendor/model`), one analyzer mode the file offers (`offered`),
/// analyzer uploads and ASTM results, and its own evidence; at most one per
/// mode. A record-only snapshot is documented, so no level above it is taken.
pub(crate) fn validate_recording(
    departures: &[MachineDeparture],
    astm: bool,
    model: Option<&str>,
    offered: &[crate::ChoiceName],
    path: &str,
) -> Result<(), Error> {
    if departures.len() > 12 {
        return Err(refusal(path, "deviations", Refusal::TooManyDeclarations));
    }
    let mut modes_seen = std::collections::HashSet::new();
    for departure in departures {
        departure.evidence_snapshot()?;
        let tag = departure.tag();
        let fail = |reason| refusal(path, tag, reason);
        let MachineDeparture::AstmPackedRecordsInFrame(declaration) = departure else {
            return Err(fail(Refusal::RecordOnlyRule));
        };
        if !astm {
            return Err(fail(Refusal::ProtocolMismatch));
        }
        let scope = &declaration.scope;
        applicability(scope).map_err(fail)?;
        if model != Some(scope.model.as_str()) {
            return Err(fail(Refusal::ScopeMismatch));
        }
        if !offered
            .iter()
            .any(|mode| mode.as_str() == scope.mode.as_str())
        {
            return Err(fail(Refusal::ModeNotOffered));
        }
        if !modes_seen.insert(scope.mode.as_str()) {
            return Err(fail(Refusal::DuplicateRule));
        }
        if scope.edition != DepartureEdition::Astm {
            return Err(fail(Refusal::EditionMismatch));
        }
        if scope.direction != DepartureDirection::MachineToHost {
            return Err(fail(Refusal::DirectionMismatch));
        }
        if scope.message != DepartureMessage::AstmResults {
            return Err(fail(Refusal::MessageMismatch));
        }
        evidence::validate(departure, path, &|_, _| false).map_err(|failure| Error::Departure {
            path: path.into(),
            rule: tag.into(),
            reason: failure.reason,
            source: failure.source,
        })?;
    }
    Ok(())
}

fn validate_scope(m: &ParsedMachine<'_>, departure: &MachineDeparture) -> Result<(), Refusal> {
    let scope = &departure.declaration().scope;
    let installed = m
        .departure_scope
        .as_ref()
        .ok_or(Refusal::MissingInstalledScope)?;
    let record_query = matches!(departure, MachineDeparture::Hl7RecordDisplayQuery(_));
    // One installed mode can upload ORUs and query orders.
    let mut expected = installed.clone();
    if record_query {
        expected.message = DepartureMessage::QryQ02;
    }
    if *scope != expected {
        return Err(Refusal::ScopeMismatch);
    }
    applicability(scope)?;
    let hl7 = match departure {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_)
        | MachineDeparture::Hl7AbsentObx11MeansFinal(_)
        | MachineDeparture::Hl7CategorizedOruResults(_)
        | MachineDeparture::Hl7RecordDisplayQuery(_)
        | MachineDeparture::Hl7Msh16MessageCategory(_)
        | MachineDeparture::Hl7Utf8InV231(_)
        | MachineDeparture::Hl7CrLfTerminators(_)
        | MachineDeparture::Hl7LfTerminators(_)
        | MachineDeparture::Hl7PreserveUnsupportedEscapes(_) => true,
        MachineDeparture::AstmAbsentH12MeansProduction(_)
        | MachineDeparture::AstmRepeatEnqAfterLostAck(_)
        | MachineDeparture::AstmPackedRecordsInFrame(_)
        | MachineDeparture::AstmFrameLimit(_)
        | MachineDeparture::AstmReceiveTimeout(_)
        | MachineDeparture::AstmSendTimeout(_)
        | MachineDeparture::AstmHostContentionPriority(_)
        | MachineDeparture::AstmLowercaseChecksumHex(_) => false,
    };
    if (hl7 && !m.choice().has_hl7_departures()) || (!hl7 && !m.choice().has_astm_records()) {
        return Err(Refusal::ProtocolMismatch);
    }
    match scope.edition {
        DepartureEdition::Hl7 { edition } if hl7 && Some(edition) == m.syntax.edition => {}
        DepartureEdition::Astm if !hl7 => {}
        DepartureEdition::Hl7 { .. } | DepartureEdition::Astm => {
            return Err(Refusal::EditionMismatch);
        }
    }
    if hl7 && scope.direction != DepartureDirection::MachineToHost {
        return Err(Refusal::DirectionMismatch);
    }
    let category = matches!(
        departure,
        MachineDeparture::Hl7Msh16MessageCategory(_)
            | MachineDeparture::Hl7CategorizedOruResults(_)
    );
    if hl7
        && scope.message
            != if record_query {
                DepartureMessage::QryQ02
            } else if category {
                DepartureMessage::OruR01
            } else {
                DepartureMessage::OruR01Patient
            }
    {
        return Err(Refusal::MessageMismatch);
    }
    if !hl7
        && !matches!(
            scope.message,
            DepartureMessage::AstmResults | DepartureMessage::AstmLink
        )
    {
        return Err(Refusal::MessageMismatch);
    }
    validate_rule(m, departure, scope)
}

fn validate_rule(
    m: &ParsedMachine<'_>,
    departure: &MachineDeparture,
    scope: &DepartureScope,
) -> Result<(), Refusal> {
    match departure {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_)
        | MachineDeparture::Hl7AbsentObx11MeansFinal(_) => {}
        MachineDeparture::Hl7Msh16MessageCategory(_) => validate_category_edition(scope.edition)?,
        MachineDeparture::Hl7RecordDisplayQuery(_) => {
            validate_category_edition(scope.edition)?;
            if !m
                .orders
                .as_ref()
                .and_then(|orders| orders.hl7.as_ref())
                .is_some_and(|layout| layout.record_location.is_some())
            {
                return Err(Refusal::InactiveSetting);
            }
        }
        MachineDeparture::Hl7CategorizedOruResults(_) => {
            validate_category_edition(scope.edition)?;
            if !m
                .deviations
                .iter()
                .any(|rule| matches!(rule, MachineDeparture::Hl7Msh16MessageCategory(_)))
            {
                return Err(Refusal::InactiveSetting);
            }
        }
        MachineDeparture::AstmPackedRecordsInFrame(_)
        | MachineDeparture::AstmAbsentH12MeansProduction(_)
        | MachineDeparture::AstmRepeatEnqAfterLostAck(_) => {
            validate_astm_upload(m, departure, scope)?;
        }
        MachineDeparture::Hl7Utf8InV231(_) => {
            if !m
                .syntax
                .edition
                .is_some_and(|edition| analyte_hl7::Version::from(edition).utf8_departure())
            {
                return Err(Refusal::EditionMismatch);
            }
            if !m.syntax.allow_utf8_in_v231 || m.syntax.encoding != Encoding::Utf8 {
                return Err(Refusal::InactiveSetting);
            }
        }
        MachineDeparture::Hl7CrLfTerminators(_)
        | MachineDeparture::Hl7LfTerminators(_)
        | MachineDeparture::Hl7PreserveUnsupportedEscapes(_)
        | MachineDeparture::AstmFrameLimit(_)
        | MachineDeparture::AstmReceiveTimeout(_)
        | MachineDeparture::AstmSendTimeout(_)
        | MachineDeparture::AstmHostContentionPriority(_)
        | MachineDeparture::AstmLowercaseChecksumHex(_) => {
            if !selected_settings(m)
                .iter()
                .any(|(tag, selected)| *tag == departure.tag() && *selected)
            {
                return Err(Refusal::InactiveSetting);
            }
        }
    }
    Ok(())
}

fn validate_astm_upload(
    machine: &ParsedMachine<'_>,
    departure: &MachineDeparture,
    scope: &DepartureScope,
) -> Result<(), Refusal> {
    if !machine.choice().has_astm_link() {
        return Err(Refusal::ProtocolMismatch);
    }
    if scope.direction != DepartureDirection::MachineToHost {
        return Err(Refusal::DirectionMismatch);
    }
    if scope.message != DepartureMessage::AstmResults
        || (matches!(departure, MachineDeparture::AstmRepeatEnqAfterLostAck(_))
            && machine
                .orders
                .as_ref()
                .is_some_and(|orders| orders.query.is_some()))
    {
        return Err(Refusal::MessageMismatch);
    }
    Ok(())
}

fn validate_category_edition(edition: DepartureEdition) -> Result<(), Refusal> {
    if matches!(edition, DepartureEdition::Hl7 { edition }
        if analyte_hl7::Version::from(edition) == analyte_hl7::Version::V231)
    {
        Ok(())
    } else {
        Err(Refusal::EditionMismatch)
    }
}
