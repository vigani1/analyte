//! Records enabled declarations and wire applications even when syntax or mapping is parked.
use crate::{Error, installation::config::Machine, invalid};
use analyte_config::{MachineDeparture, RecordTransport, Stack};
use analyte_model::{
    AppliedDeparture, DepartureEvidence, DepartureMeaning, DepartureSpan, EvidenceList, Identifier,
    SourceText,
};

pub(crate) async fn for_receipt(
    store: &analyte_store::Handle,
    machine: &Machine,
    id: analyte_model::ReceiptId,
    mapping: Result<analyte_mapping::Output, Error>,
) -> Result<(Result<analyte_mapping::Output, Error>, DepartureEvidence), Error> {
    let receipt = store
        .request(crate::deadline(), move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    let applied = mapping
        .as_ref()
        .map_or(&[][..], |output| output.applied_departures.as_slice());
    let mut evidence = collect(machine, &receipt.raw, applied)?;
    let preserved = mapping.as_ref().is_ok_and(|output| {
        output
            .findings
            .iter()
            .any(|f| f.problem == analyte_mapping::Problem::UnsupportedEscape)
    });
    let mut entries = evidence.applied.as_slice().to_vec();
    if let Err(error) = compatibility(machine, &receipt.raw, preserved, &mut entries) {
        evidence.incomplete = Some(analyte_model::Reason::new(&format!(
            "departure-evidence failure: {error}"
        ))?);
    }
    evidence.applied = EvidenceList::new(entries)?;
    let mapping = if let Some(reason) = &evidence.incomplete
        && mapping.is_ok()
    {
        Err(invalid(reason.as_str()))
    } else {
        mapping
    };
    Ok((mapping, evidence))
}

pub(crate) fn collect(
    machine: &Machine,
    raw: &[u8],
    applied: &[AppliedDeparture],
) -> Result<DepartureEvidence, Error> {
    match try_collect(machine, raw, applied) {
        Ok(evidence) => Ok(evidence),
        Err(error) => Ok(DepartureEvidence {
            enabled: EvidenceList::new(
                machine
                    .snapshot
                    .effective()
                    .machine()
                    .deviations
                    .iter()
                    .map(MachineDeparture::evidence_snapshot)
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
            applied: EvidenceList::new(applied.to_vec())?,
            incomplete: Some(analyte_model::Reason::new(&format!(
                "departure-evidence failure: {error}"
            ))?),
        }),
    }
}

fn try_collect(
    machine: &Machine,
    raw: &[u8],
    applied: &[AppliedDeparture],
) -> Result<DepartureEvidence, Error> {
    let declarations = &machine.snapshot.effective().machine().deviations;
    let enabled = declarations
        .iter()
        .map(MachineDeparture::evidence_snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    let mut entries = applied.to_vec();
    if let Stack::Astm(RecordTransport::Stream(settings)) = machine.snapshot.effective().stack()
        && let Some(declaration) = declarations
            .iter()
            .find(|d| matches!(d, MachineDeparture::AstmPackedRecordsInFrame(_)))
    {
        packed(raw, settings, declaration, &mut entries)?;
    }
    Ok(DepartureEvidence {
        enabled: EvidenceList::new(enabled)?,
        applied: EvidenceList::new(entries)?,
        incomplete: None,
    })
}

fn packed(
    raw: &[u8],
    settings: &analyte_config::AstmStack,
    declaration: &MachineDeparture,
    entries: &mut Vec<AppliedDeparture>,
) -> Result<(), Error> {
    use analyte_framing::astm::{Decoder, Event, Limit};
    let mut decoder = Decoder::new(
        Limit::new(settings.frame_limit())?,
        settings.checksum_policy(),
    );
    let mut record = 1usize;
    let mut spans = Vec::new();
    let mut continued_packing = false;
    for (index, byte) in raw.iter().enumerate() {
        if let Some(Event::Frame(frame)) = decoder.push(*byte) {
            let start = index
                .saturating_add(1)
                .checked_sub(frame.raw().len())
                .ok_or_else(|| invalid("ASTM evidence frame span underflow"))?;
            let text_start = start
                .checked_add(2)
                .ok_or_else(|| invalid("ASTM evidence frame offset overflow"))?;
            let crs = frame.text().iter().filter(|b| **b == b'\r').count();
            let nonstandard =
                crs > 1 || (crs > 0 && (!frame.is_final() || frame.text().last() != Some(&b'\r')));
            // Each entry links source record fragments to the original verified frame bytes.
            let mut fragment_start = text_start;
            for (offset, byte) in frame.text().iter().enumerate() {
                let end = text_start
                    .checked_add(offset)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| invalid("ASTM record span overflow"))?;
                if *byte == b'\r' {
                    spans.push(DepartureSpan::try_from(fragment_start..end).map_err(invalid)?);
                    if nonstandard || continued_packing {
                        if entries.len() == 256 {
                            return Err(invalid("applied departure evidence limit"));
                        }
                        entries.push(AppliedDeparture {
                            rule:Identifier::new(declaration.tag())?,declaration:declaration.evidence_snapshot()?,
                            standard:Identifier::new("LIS01-A2 §§3.3.2, 6.3.1.1, 8.3.1.1")?,record,field:0,
                            source:SourceText::new("CR-ended records share a verified source frame; no semantic values derived")?,
                            meaning:DepartureMeaning::PackedRecords,coordinates:Identifier::new("receipt_bytes")?,
                            spans:EvidenceList::new(std::mem::take(&mut spans))?,
                        });
                    } else {
                        spans.clear();
                    }
                    continued_packing = false;
                    record = record
                        .checked_add(1)
                        .ok_or_else(|| invalid("ASTM record count overflow"))?;
                    fragment_start = end;
                }
            }
            let text_end = text_start
                .checked_add(frame.text().len())
                .ok_or_else(|| invalid("ASTM frame span overflow"))?;
            if fragment_start < text_end {
                continued_packing |= nonstandard;
                spans.push(DepartureSpan::try_from(fragment_start..text_end).map_err(invalid)?);
            }
        }
    }
    Ok(())
}

fn compatibility(
    machine: &Machine,
    raw: &[u8],
    preserved_escape: bool,
    entries: &mut Vec<AppliedDeparture>,
) -> Result<(), Error> {
    let effective = machine.snapshot.effective();
    let hl7 = if effective.hl7_options().is_some() {
        analyte_mapping::read::payload(&machine.snapshot, raw)
            .ok()
            .and_then(|bytes| {
                analyte_hl7::Message::parse(&bytes, effective.hl7_options()?)
                    .ok()
                    .map(|message| {
                        let utf8 = message
                            .segments()
                            .next()
                            .and_then(|msh| msh.field(18))
                            .is_some_and(|f| f.raw() == b"UNICODE UTF-8");
                        (
                            message.raw().windows(2).any(|bytes| bytes == b"\r\n"),
                            message.raw().contains(&b'\n'),
                            utf8,
                        )
                    })
            })
    } else {
        None
    };
    for declaration in &effective.machine().deviations {
        let (exercised, standard) = match declaration {
            MachineDeparture::Hl7CrLfTerminators(_) => {
                (hl7.is_some_and(|v| v.0), "HL7 §2.4 segment boundaries")
            }
            MachineDeparture::Hl7LfTerminators(_) => {
                (hl7.is_some_and(|v| v.1), "HL7 §2.4 segment boundaries")
            }
            MachineDeparture::Hl7Utf8InV231(_) => {
                (hl7.is_some_and(|v| v.2), "HL7 2.3.1 §2.24.1.18")
            }
            MachineDeparture::Hl7PreserveUnsupportedEscapes(_) => {
                (preserved_escape, "HL7 §2.7 escape sequences")
            }
            MachineDeparture::AstmLowercaseChecksumHex(_) => {
                (lowercase_checksum(machine, raw)?, "LIS01-A2 §§6.3.3, 8.3")
            }
            MachineDeparture::Hl7AbsentObr25MeansFinal(_)
            | MachineDeparture::Hl7AbsentObx11MeansFinal(_)
            | MachineDeparture::AstmPackedRecordsInFrame(_)
            | MachineDeparture::AstmFrameLimit(_)
            | MachineDeparture::AstmReceiveTimeout(_)
            | MachineDeparture::AstmSendTimeout(_)
            | MachineDeparture::AstmHostContentionPriority(_) => (false, ""),
        };
        if exercised {
            if entries.len() == 256 {
                return Err(invalid("applied departure evidence limit"));
            }
            entries.push(AppliedDeparture {
                rule:Identifier::new(declaration.tag())?,declaration:declaration.evidence_snapshot()?,standard:Identifier::new(standard)?,
                record:0,field:0,source:SourceText::new("Declared wire compatibility exercised; original bytes and semantic values retained")?,
                meaning:DepartureMeaning::WireCompatibility,coordinates:Identifier::new("receipt_bytes")?,
                spans:EvidenceList::new(vec![DepartureSpan::try_from(0..raw.len()).map_err(invalid)?])?,
            });
        }
    }
    Ok(())
}

fn lowercase_checksum(machine: &Machine, raw: &[u8]) -> Result<bool, Error> {
    use analyte_framing::astm::{Decoder, Event, Limit};
    let Stack::Astm(RecordTransport::Stream(settings)) = machine.snapshot.effective().stack()
    else {
        return Ok(false);
    };
    let mut decoder = Decoder::new(
        Limit::new(settings.frame_limit())?,
        settings.checksum_policy(),
    );
    for byte in raw {
        if let Some(Event::Frame(frame)) = decoder.push(*byte)
            && frame
                .raw()
                .get(frame.raw().len().saturating_sub(4)..frame.raw().len().saturating_sub(2))
                .is_some_and(|digits| digits.iter().any(|b| matches!(*b, b'a'..=b'f')))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
