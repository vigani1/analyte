//! Applies only evidenced missing-status interpretations after strict report diagnostics exist.
use crate::{Error, Output, Position, Problem, finding, identifier, text};
use analyte_config::MachineDeparture;
use analyte_hl7::{Message, Msh};
use analyte_model::{
    AppliedDeparture, DepartureMeaning, DepartureSpan, EvidenceList, Identifier, QcClass,
    ResultStatus,
};

pub(super) fn apply(
    message: &Message,
    declarations: &[MachineDeparture],
    output: &mut Output,
) -> Result<(), Error> {
    let msh = Msh::new(message).map_err(|source| Error::Hl7 {
        position: Position {
            record: 1,
            field: 9,
        },
        source,
    })?;
    if !msh
        .message_type()
        .take(2)
        .eq([b"ORU".as_slice(), b"R01".as_slice()])
    {
        return Ok(());
    }
    let obr = declarations
        .iter()
        .find(|d| matches!(d, MachineDeparture::Hl7AbsentObr25MeansFinal(_)));
    let obx = declarations
        .iter()
        .find(|d| matches!(d, MachineDeparture::Hl7AbsentObx11MeansFinal(_)));
    let mut offset = 0usize;
    let mut report_patient = false;
    for (index, segment) in message.segments().enumerate() {
        let record = index.saturating_add(1);
        let end = offset
            .checked_add(segment.raw().len())
            .ok_or(Error::Limit {
                position: Position { record, field: 0 },
                resource: "source byte span",
            })?;
        if segment.id() == b"OBR" {
            report_patient = patient_scope(output, record, "OBR");
        }
        let (selected, field) = match segment.id() {
            b"OBR" if report_patient => (obr, 25),
            b"OBX" if report_patient && patient_scope(output, record, "OBX") => (obx, 11),
            _ => (None, 0),
        };
        if let Some(declaration) = selected {
            let position = Position { record, field };
            let raw = segment.field(field).map_or(&[][..], |f| f.raw());
            if raw.is_empty()
                && (field != 25 || report_can_be_final(message, record, obx.is_some()))
            {
                discharge(output, declaration, position, offset..end)?;
            } else if !raw.is_empty() && raw != b"F" {
                finding(output, position, Problem::DepartureConflict)?;
            }
        }
        // A partial/failed observation cannot substantiate a final-only report guarantee.
        if obr.is_some() && report_patient && segment.id() == b"OBX" {
            let status = segment.field(11).map_or(&[][..], |f| f.raw());
            if !status.is_empty() && status != b"F" {
                finding(
                    output,
                    Position { record, field: 11 },
                    Problem::DepartureConflict,
                )?;
            }
        }
        offset = end;
        // Segment spans are in syntax-byte coordinates, including the original negotiated delimiters.
        while message
            .raw()
            .get(offset)
            .is_some_and(|b| matches!(*b, b'\r' | b'\n'))
        {
            offset = offset.saturating_add(1);
        }
    }
    Ok(())
}

// This only rejects contradictory/unknown observation evidence. The documented
// OBR omission guarantee, never a count of received observations, supplies completion.
fn report_can_be_final(message: &Message, record: usize, missing_observation_final: bool) -> bool {
    for segment in message.segments().skip(record) {
        if segment.id() == b"OBR" {
            break;
        }
        if segment.id() == b"OBX" {
            let status = segment.field(11).map_or(&[][..], |field| field.raw());
            if status != b"F" && !(status.is_empty() && missing_observation_final) {
                return false;
            }
        }
    }
    true
}

fn patient_scope(output: &Output, record: usize, kind: &str) -> bool {
    let mut found = false;
    for part in output
        .results
        .iter()
        .flat_map(|r| r.content.parts.as_slice())
    {
        if part
            .source_path
            .as_slice()
            .iter()
            .any(|n| n.occurrence == record && n.kind.as_str() == kind)
        {
            found = true;
            if part.qc_class != QcClass::Patient {
                return false;
            }
        }
    }
    found
}

fn discharge(
    output: &mut Output,
    declaration: &MachineDeparture,
    position: Position,
    span: std::ops::Range<usize>,
) -> Result<(), Error> {
    if output.applied_departures.len() == 256 {
        return Err(Error::Limit {
            position,
            resource: "applied departures",
        });
    }
    let meaning = match declaration {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_) => {
            DepartureMeaning::FinalReportFromDocumentation
        }
        MachineDeparture::Hl7AbsentObx11MeansFinal(_) => {
            DepartureMeaning::FinalObservationFromDocumentation
        }
        MachineDeparture::AstmPackedRecordsInFrame(_)
        | MachineDeparture::Hl7Utf8InV231(_)
        | MachineDeparture::Hl7CrLfTerminators(_)
        | MachineDeparture::Hl7LfTerminators(_)
        | MachineDeparture::Hl7PreserveUnsupportedEscapes(_)
        | MachineDeparture::AstmFrameLimit(_)
        | MachineDeparture::AstmReceiveTimeout(_)
        | MachineDeparture::AstmSendTimeout(_)
        | MachineDeparture::AstmHostContentionPriority(_)
        | MachineDeparture::AstmLowercaseChecksumHex(_) => return Err(Error::DepartureKind),
    };
    let spans = EvidenceList::new(vec![
        DepartureSpan::try_from(span).map_err(|reason| Error::Structure { position, reason })?,
    ])
    .map_err(|_| Error::Limit {
        position,
        resource: "source spans",
    })?;
    let evidence = AppliedDeparture {
        rule: identifier(declaration.tag(), position)?,
        declaration: declaration.evidence_snapshot()?,
        standard: Identifier::new(if position.field == 25 {
            "HL7 §7.4.1.25 (2.3.1 §7.3.2)"
        } else {
            "HL7 §7.4.2.11 (2.3.1 §7.3.2)"
        })
        .map_err(|source| Error::Text { position, source })?,
        record: position.record,
        field: position.field,
        source: text("absent or empty; source unchanged", position)?,
        meaning,
        coordinates: identifier("syntax_bytes", position)?,
        spans,
    };
    if meaning == DepartureMeaning::FinalObservationFromDocumentation {
        for result in &mut output.results {
            let mut parts = result.content.parts.as_slice().to_vec();
            for part in &mut parts {
                if part
                    .source_path
                    .as_slice()
                    .last()
                    .is_some_and(|n| n.kind.as_str() == "OBX" && n.occurrence == position.record)
                {
                    part.status = ResultStatus::Final;
                }
            }
            result.content.parts = analyte_model::NonEmptyEvidenceList::new(crate::list(
                parts, position,
            )?)
            .map_err(|_| Error::Limit {
                position,
                resource: "result parts",
            })?;
        }
    }
    output
        .findings
        .retain(|f| !(f.position == position && f.problem == Problem::UnqualifiedHl7Report));
    output.applied_departures.push(evidence);
    finding(
        output,
        position,
        Problem::DepartureDerivedStatusRequiresContract,
    )
}
