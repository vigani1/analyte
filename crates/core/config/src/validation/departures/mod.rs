//! Checks named departures against installed applicability and selected settings.
use super::super::*;
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
    m: &MachineDocument,
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
        evidence::validate(m, departure, reference_path).map_err(|failure| Error::Departure {
            path: path.into(),
            rule: tag.into(),
            reason: failure.reason,
            source: failure.source,
        })?;
    }
    for (tag, selected) in selected_settings(m) {
        if selected && !tags.contains(tag) {
            return Err(refusal(path, tag, Refusal::MissingDeclaration));
        }
    }
    Ok(())
}

fn selected_settings(m: &MachineDocument) -> [(&'static str, bool); 9] {
    let s = &m.syntax;
    let astm = m.framing.kind == FramingKind::AstmLis01;
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

fn validate_scope(m: &MachineDocument, departure: &MachineDeparture) -> Result<(), Refusal> {
    let scope = &departure.declaration().scope;
    let installed = m
        .departure_scope
        .as_ref()
        .ok_or(Refusal::MissingInstalledScope)?;
    if scope != installed {
        return Err(Refusal::ScopeMismatch);
    }
    for text in [&scope.model, &scope.mode, &scope.firmware] {
        if text.as_str().contains(['*', '?']) || matches!(text.as_str(), "all" | "any" | "untested")
        {
            return Err(Refusal::UnsupportedApplicability);
        }
    }
    if scope.model.as_str() == "unknown" || scope.mode.as_str() == "unknown" {
        return Err(Refusal::UnsupportedApplicability);
    }
    let hl7 = match departure {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_)
        | MachineDeparture::Hl7AbsentObx11MeansFinal(_)
        | MachineDeparture::Hl7Utf8InV231(_)
        | MachineDeparture::Hl7CrLfTerminators(_)
        | MachineDeparture::Hl7LfTerminators(_)
        | MachineDeparture::Hl7PreserveUnsupportedEscapes(_) => true,
        MachineDeparture::AstmPackedRecordsInFrame(_)
        | MachineDeparture::AstmFrameLimit(_)
        | MachineDeparture::AstmReceiveTimeout(_)
        | MachineDeparture::AstmSendTimeout(_)
        | MachineDeparture::AstmHostContentionPriority(_)
        | MachineDeparture::AstmLowercaseChecksumHex(_) => false,
    };
    if (hl7 && m.protocol != Protocol::Hl7) || (!hl7 && m.protocol != Protocol::Astm) {
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
    if hl7 && scope.message != DepartureMessage::OruR01Patient {
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
    match departure {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_)
        | MachineDeparture::Hl7AbsentObx11MeansFinal(_) => {}
        MachineDeparture::AstmPackedRecordsInFrame(_) => {
            if m.framing.kind != FramingKind::AstmLis01 {
                return Err(Refusal::ProtocolMismatch);
            }
            if scope.direction != DepartureDirection::MachineToHost {
                return Err(Refusal::DirectionMismatch);
            }
            if scope.message != DepartureMessage::AstmResults {
                return Err(Refusal::MessageMismatch);
            }
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
