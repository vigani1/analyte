//! Resolves bounded local recording references and checks per-rule evidence prerequisites.
use super::{
    DepartureDirection, DepartureEdition, DepartureGuarantee, DepartureReference, DepartureScope,
    EvidenceLevel, MachineDeparture, Refusal,
};
use crate::ParsedMachine;
use analyte_transport::recording::Evidence as RecordingEvidence;

pub(super) struct Failure {
    pub(super) reason: Refusal,
    pub(super) source: Option<Box<dyn std::error::Error + Send + Sync>>,
}
impl From<Refusal> for Failure {
    fn from(reason: Refusal) -> Self {
        Self {
            reason,
            source: None,
        }
    }
}
fn io(reason: Refusal, source: std::io::Error) -> Failure {
    Failure {
        reason,
        source: Some(Box::new(source)),
    }
}

use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path},
};

/// Checks one declaration's evidence; `exercised` says whether a recording
/// file exercised the declared capability, which a simulated or proven level
/// needs.
pub(super) fn validate(
    departure: &MachineDeparture,
    path: &str,
    exercised: &dyn Fn(&MachineDeparture, &analyte_model::Identifier) -> bool,
) -> Result<(), Failure> {
    let declaration = departure.declaration();
    if declaration.evidence.is_empty() {
        return Err(Refusal::EmptyEvidence.into());
    }
    if declaration.evidence.len() > 16 {
        return Err(Refusal::EvidenceLimit.into());
    }
    let required = match departure {
        MachineDeparture::Hl7AbsentObr25MeansFinal(_) => DepartureGuarantee::AbsentObr25Final,
        MachineDeparture::Hl7AbsentObx11MeansFinal(_) => DepartureGuarantee::AbsentObx11Final,
        MachineDeparture::AstmAbsentH12MeansProduction(_) => {
            DepartureGuarantee::AbsentProcessingIdProduction
        }
        MachineDeparture::AstmRepeatEnqAfterLostAck(_) => DepartureGuarantee::RepeatEnq,
        MachineDeparture::AstmPackedRecordsInFrame(_) => DepartureGuarantee::PackedRecords,
        MachineDeparture::Hl7Utf8InV231(_)
        | MachineDeparture::Hl7CrLfTerminators(_)
        | MachineDeparture::Hl7LfTerminators(_)
        | MachineDeparture::Hl7PreserveUnsupportedEscapes(_)
        | MachineDeparture::AstmFrameLimit(_)
        | MachineDeparture::AstmReceiveTimeout(_)
        | MachineDeparture::AstmSendTimeout(_)
        | MachineDeparture::AstmHostContentionPriority(_)
        | MachineDeparture::AstmLowercaseChecksumHex(_) => DepartureGuarantee::Compatibility,
    };
    let mut manual = false;
    let mut unknown_covered = false;
    let mut simulated = false;
    let mut proven = false;
    let mut packing = false;
    for reference in &declaration.evidence {
        match reference {
            DepartureReference::Manual {
                reference,
                title,
                revision,
                section,
                guarantee,
                covers_unknown_firmware,
                contradicts,
            } => {
                if *contradicts {
                    return Err(Refusal::ContradictoryEvidence.into());
                }
                if [reference, title, revision, section].iter().any(|value| {
                    value.as_str().trim().is_empty()
                        || matches!(value.as_str(), "unknown" | "untested")
                }) || reference.as_str().contains(['/', '\\', ':'])
                {
                    return Err(Refusal::InvalidManualReference.into());
                }
                manual |= *guarantee == required;
                unknown_covered |= *covers_unknown_firmware && *guarantee == required;
            }
            DepartureReference::Recording {
                file,
                sha256,
                first_event,
                last_event,
            } => {
                let origin = recording(
                    file.as_str(),
                    sha256.as_str(),
                    *first_event,
                    *last_event,
                    &declaration.scope,
                    path,
                )?;
                packing |= origin != RecordingEvidence::Synthetic;
                let capability = exercised(departure, file);
                simulated |= capability
                    && matches!(
                        origin,
                        RecordingEvidence::Simulator | RecordingEvidence::VendorTrace
                    );
                proven |= capability && origin == RecordingEvidence::Machine;
            }
        }
    }
    if !manual && !(required == DepartureGuarantee::PackedRecords && packing) {
        return Err(Refusal::MissingRuleEvidence.into());
    }
    if declaration.scope.firmware.as_str() == "unknown" && !unknown_covered {
        return Err(Refusal::UnknownFirmware.into());
    }
    match declaration.level {
        EvidenceLevel::Documented => {}
        EvidenceLevel::Simulated if simulated => {}
        EvidenceLevel::Proven if proven => {}
        EvidenceLevel::Simulated | EvidenceLevel::Proven => {
            return Err(Refusal::LevelEvidence.into());
        }
    }
    Ok(())
}

pub(super) fn exercised_capability(
    m: &ParsedMachine<'_>,
    departure: &MachineDeparture,
    file: &analyte_model::Identifier,
) -> bool {
    m.evidence.capabilities.iter().any(|c| {
        c.model == departure.declaration().scope.model
            && c.firmware == departure.declaration().scope.firmware
            && c.transport == m.transport.kind
            && c.config_digest.as_str().len() == 64
            && c.config_digest
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && !matches!(c.driver.as_str(), "untested" | "unknown")
            && !matches!(c.platform.as_str(), "untested" | "unknown")
            && c.evidence_files.contains(file)
            && c.dialogs
                .iter()
                .any(|dialog| dialog.as_str() == departure.tag())
    })
}

fn recording(
    file: &str,
    digest: &str,
    first: u32,
    last: u32,
    scope: &DepartureScope,
    source: &str,
) -> Result<RecordingEvidence, Failure> {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let relative = Path::new(file);
    if source.starts_with("\\\\")
        || source.starts_with("//")
        || !file.ends_with(".rec.jsonl")
        || file.contains(['\\', ':'])
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Refusal::InvalidLocalReference.into());
    }
    let base = Path::new(source)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let base = base
        .canonicalize()
        .map_err(|source| io(Refusal::UnresolvedRecording, source))?;
    let resolved = base
        .join(relative)
        .canonicalize()
        .map_err(|source| io(Refusal::UnresolvedRecording, source))?;
    if !resolved.starts_with(&base) {
        return Err(Refusal::InvalidLocalReference.into());
    }
    let input =
        std::fs::File::open(resolved).map_err(|source| io(Refusal::UnresolvedRecording, source))?;
    if !input
        .metadata()
        .map_err(|source| io(Refusal::RecordingRead, source))?
        .is_file()
    {
        return Err(Refusal::UnresolvedRecording.into());
    }
    // Bound actual reads too: metadata alone cannot constrain a growing file.
    let mut bytes = Vec::new();
    input
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| io(Refusal::RecordingRead, source))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Refusal::RecordingLimit.into());
    }
    let expected = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != expected {
        return Err(Refusal::DigestMismatch.into());
    }
    recording_content(&bytes, first, last, scope)
}

fn recording_content(
    bytes: &[u8],
    first: u32,
    last: u32,
    scope: &DepartureScope,
) -> Result<RecordingEvidence, Failure> {
    use analyte_transport::recording::{self, Direction, Limits};
    let (header, events) = recording::read(bytes, Limits::default()).map_err(|source| Failure {
        reason: if source.reason.contains("limit") {
            Refusal::RecordingLimit
        } else {
            Refusal::MalformedRecording
        },
        source: Some(Box::new(source)),
    })?;
    let syntax = match scope.edition {
        DepartureEdition::Hl7 { .. } => "hl7_v2",
        DepartureEdition::Astm => "astm_lis02",
    };
    if header.stack.get(3).map(String::as_str) != Some(syntax)
        || header.machine.as_deref() != Some(scope.model.as_str())
        || !(header.software.as_deref() == Some(scope.firmware.as_str())
            || (scope.firmware.as_str() == "unknown" && header.software.is_none()))
    {
        return Err(Refusal::RecordingScopeMismatch.into());
    }
    let direction = match scope.direction {
        DepartureDirection::MachineToHost => Direction::MachineToHost,
        DepartureDirection::HostToMachine => Direction::HostToMachine,
    };
    let start = first
        .checked_sub(1)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(Refusal::ExchangeMismatch)?;
    let end = usize::try_from(last).map_err(|_| Refusal::ExchangeMismatch)?;
    let exchange = events
        .get(start..end)
        .filter(|items| !items.is_empty())
        .ok_or(Refusal::ExchangeMismatch)?;
    if !exchange.iter().any(|event| {
        event.direction == direction && !event.bytes.is_empty() && event.link.is_none()
    }) {
        return Err(Refusal::ExchangeMismatch.into());
    }
    Ok(header.evidence)
}
