//! Records enabled declarations and wire applications even when syntax or mapping is parked.
use crate::installation::config::Machine;
use crate::intake::{Mapping, ParkReason};
use EvidenceError as Error;
use analyte_config::{MachineDeparture, RecordTransport, Stack};
use analyte_model::{
    AppliedDeparture, DepartureEvidence, DepartureMeaning, DepartureSpan, EvidenceList, Identifier,
    SourceText,
};

pub(crate) fn for_receipt(
    machine: &Machine,
    receipt: &analyte_store::Receipt,
    wire: Option<&analyte_hl7::WireCompatibility>,
    mapping: Mapping,
) -> Result<(Mapping, DepartureEvidence), Error> {
    let applied = mapping
        .output()
        .map_or(&[][..], |output| output.applied_departures.as_slice());
    let mut evidence = collect(machine, &receipt.raw, applied)?;
    let preserved = mapping.output().is_some_and(|output| {
        output
            .findings
            .iter()
            .any(|f| f.problem == analyte_mapping::Problem::UnsupportedEscape)
    });
    let mut entries = evidence.applied.as_slice().to_vec();
    if let Err(error) = compatibility(machine, &receipt.raw, wire, preserved, &mut entries) {
        evidence.incomplete = Some(analyte_model::Reason::new(&format!(
            "departure-evidence failure: {error}"
        ))?);
    }
    evidence.applied = EvidenceList::new(entries)?;
    let mapping = if let Some(reason) = &evidence.incomplete
        && mapping.output().is_some()
    {
        Mapping::Parked(ParkReason::IncompleteDepartures(reason.clone()))
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
        Limit::new(settings.settings().frame_limit())?,
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
                .ok_or_else(|| Error::AstmEvidenceFrameSpanUnderflow)?;
            let nonstandard = frame.has_packed_records();
            for (fragment, ends_record) in frame.record_fragments() {
                let fragment_start = start
                    .checked_add(fragment.start)
                    .ok_or(Error::AstmRecordSpanOverflow)?;
                let end = start
                    .checked_add(fragment.end)
                    .ok_or(Error::AstmRecordSpanOverflow)?;
                spans.push(
                    DepartureSpan::try_from(fragment_start..end)
                        .map_err(|_| Error::ReversedSpan)?,
                );
                if ends_record {
                    if nonstandard || continued_packing {
                        if entries.len() == 256 {
                            return Err(Error::AppliedDepartureEvidenceLimit);
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
                        .ok_or(Error::AstmRecordCountOverflow)?;
                } else {
                    continued_packing |= nonstandard;
                }
            }
        }
    }
    Ok(())
}

fn compatibility(
    machine: &Machine,
    raw: &[u8],
    hl7: Option<&analyte_hl7::WireCompatibility>,
    preserved_escape: bool,
    entries: &mut Vec<AppliedDeparture>,
) -> Result<(), Error> {
    let effective = machine.snapshot.effective();
    for declaration in &effective.machine().deviations {
        let (exercised, standard) = match declaration {
            MachineDeparture::Hl7CrLfTerminators(_) => (
                hl7.as_ref().is_some_and(|v| v.crlf),
                "HL7 §2.4 segment boundaries",
            ),
            MachineDeparture::Hl7LfTerminators(_) => (
                hl7.as_ref().is_some_and(|v| v.lf),
                "HL7 §2.4 segment boundaries",
            ),
            MachineDeparture::Hl7Utf8InV231(_) => (
                hl7.as_ref().is_some_and(|v| v.legacy_utf8),
                "HL7 2.3.1 §2.24.1.18",
            ),
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
                return Err(Error::AppliedDepartureEvidenceLimit);
            }
            entries.push(AppliedDeparture {
                rule:Identifier::new(declaration.tag())?,declaration:declaration.evidence_snapshot()?,standard:Identifier::new(standard)?,
                record:0,field:0,source:SourceText::new("Declared wire compatibility exercised; original bytes and semantic values retained")?,
                meaning:DepartureMeaning::WireCompatibility,coordinates:Identifier::new("receipt_bytes")?,
                spans:EvidenceList::new(vec![DepartureSpan::try_from(0..raw.len()).map_err(|_| Error::ReversedSpan)?])?,
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
        Limit::new(settings.settings().frame_limit())?,
        settings.checksum_policy(),
    );
    for byte in raw {
        if let Some(Event::Frame(frame)) = decoder.push(*byte)
            && frame.has_lowercase_checksum()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Failures constructing complete departure evidence; original source bytes remain retained.
#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    /// Source span coordinates are reversed.
    #[error("departure span end precedes start")]
    ReversedSpan,
    /// A bounded evidence list refused an entry.
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    /// Evidence cannot be represented within its declared bounds.
    #[error("ASTM evidence frame offset overflow")]
    AstmEvidenceFrameOffsetOverflow,
    /// Evidence cannot be represented within its declared bounds.
    #[error("ASTM evidence frame span underflow")]
    AstmEvidenceFrameSpanUnderflow,
    /// Evidence cannot be represented within its declared bounds.
    #[error("ASTM frame span overflow")]
    AstmFrameSpanOverflow,
    /// Evidence cannot be represented within its declared bounds.
    #[error("ASTM record count overflow")]
    AstmRecordCountOverflow,
    /// Evidence cannot be represented within its declared bounds.
    #[error("ASTM record span overflow")]
    AstmRecordSpanOverflow,
    /// Evidence cannot be represented within its declared bounds.
    #[error("applied departure evidence limit")]
    AppliedDepartureEvidenceLimit,
    /// Preserves the original evidence failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original evidence failure.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    /// Preserves the original evidence failure.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// Preserves the original evidence failure.
    #[error(transparent)]
    Astm(#[from] analyte_framing::astm::Error),
}

crate::runtime::service::conversions::causes!(EvidenceError; store);
