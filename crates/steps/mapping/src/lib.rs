//! Maps explicit machine and model terms without I/O or clinical interpretation.
//! Protocol crates own grammar and dialogs; this crate owns translations and performs no I/O.
//! The caller retains the receipt and parks any output with findings.
#![forbid(unsafe_code)]

/// ASTM terms to common results and explicit outgoing orders.
pub mod astm;
/// Autolyser terms to common evidence and explicit outgoing orders.
pub mod autolyser;
/// HL7 terms to common results and explicit outgoing orders.
pub mod hl7;

/// LAW terms to common results and explicit outgoing work.
pub mod law;

/// POCT terms to common observations and configured results.
pub mod poct;

/// VIDAS terms to common evidence and explicit outgoing orders.
pub mod vidas;
pub use astm::map as map_astm;
pub use autolyser::map as map_autolyser;
pub use hl7::map as map_hl7;
pub use inbound::evidence::ResultMappingError;
pub use law::map as map_law;
pub use orders::codes::{
    map_priority as outbound_priority, map_specimen as outbound_specimen, map_test as outbound_test,
};
pub use poct::map as map_poct;
pub use vidas::{
    ResultEvidence as VidasResultEvidence, map as map_vidas, map_evidence as vidas_evidence,
};

mod evidence_display;
mod inbound;
mod orders;

pub use orders::{
    AppOrder, AppOrderPriority, Hl7Command, LawCommand, MachineOrder, OrderMappingError,
    OutboundItem, WireMappingError, app_order, law_address, map_display, map_law_work, map_order,
};

use analyte_config::StackChoice;
use analyte_model::{
    AppliedDeparture, DepartureMeaning, EvidenceList, Identifier, ReceiptId, ResultContent,
    ResultValue, Revision, SourceNode, SourceText, TextError,
};

/// Record syntaxes supported by field-position mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSyntax {
    /// ASTM LIS02 records.
    AstmLis02,
    /// HL7 segments.
    Hl7V2,
}

/// Vendor syntax must use its own parser and model mapping.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not a field-position syntax")]
pub struct UnsupportedFieldSyntax(pub StackChoice);

impl TryFrom<StackChoice> for FieldSyntax {
    type Error = UnsupportedFieldSyntax;

    fn try_from(syntax: StackChoice) -> Result<Self, Self::Error> {
        match syntax {
            StackChoice::AstmLis02 | StackChoice::FileAstm => Ok(Self::AstmLis02),
            StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager => Ok(Self::Hl7V2),
            StackChoice::Poct | StackChoice::VidasLiteral | StackChoice::Autolyser => {
                Err(UnsupportedFieldSyntax(syntax))
            }
        }
    }
}

struct RecordSettings<'a> {
    mappings: &'a analyte_config::Mappings,
    limits: &'a analyte_config::SyntaxLimits,
}

/// Finds one exact test translation; ambiguous or absent matches remain unknown.
#[must_use]
pub fn test_translation<'a>(
    tables: &'a [analyte_config::TestMapping],
    codes: &[&str],
    system: Option<&str>,
) -> Option<&'a analyte_config::TestMapping> {
    let mut matches = tables.iter().filter(|entry| {
        codes.contains(&entry.machine_code.as_str())
            && system.is_none_or(|s| s == entry.machine_system.as_str())
    });
    let first = matches.next();
    if matches.next().is_some() {
        None
    } else {
        first
    }
}

/// Finds an exact original-unit translation; no spelling normalization occurs.
#[must_use]
pub fn unit_translation<'a>(
    tables: &'a [analyte_config::UnitMapping],
    original: &str,
) -> Option<&'a analyte_config::UnitMapping> {
    tables
        .iter()
        .find(|entry| entry.original.as_str() == original)
}

/// One-based source record/segment and field; field zero denotes the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// One-based occurrence in the complete message.
    pub record: usize,
    /// Standard field position, zero for whole-record failures.
    pub field: usize,
}

/// Mapping failure: keep the raw receipt and park it without delivering partial output.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A validated declaration could not be represented as bounded provenance.
    #[error("{0}")]
    DepartureEvidence(#[from] analyte_config::Error),
    /// Parsed bytes use a different character set from the selected protocol settings.
    #[error("parsed message character set disagrees with selected settings")]
    CharsetMismatch,
    /// A departure application selected a declaration with another meaning.
    #[error("departure declaration does not describe a missing final status")]
    DepartureKind,
    /// A validated LAW message lacks its header.
    #[error("LAW message header missing")]
    MissingHeader,
    /// A LAW notice references an order outside its original message.
    #[error("LAW notice order is not in the original message")]
    NoticeOrder,
    /// A one-based LAW notice position cannot identify a source record.
    #[error("LAW notice position is zero")]
    NoticePosition,
    /// A bound was reached before producing a partial success.
    #[error("{position}: {resource} limit exceeded")]
    Limit {
        /// Source location.
        position: Position,
        /// Exhausted bound.
        resource: &'static str,
    },
    /// Source structure cannot safely establish field ownership.
    #[error("{position}: {reason}")]
    Structure {
        /// Source location.
        position: Position,
        /// Specific structural failure.
        reason: &'static str,
    },
    /// Bytes cannot be represented as source text in the declared encoding.
    #[error("{position}: invalid source encoding")]
    Encoding {
        /// Source location.
        position: Position,
    },
    /// A common-model text constraint failed.
    #[error("{position}: {source}")]
    Text {
        /// Source location.
        position: Position,
        /// Exact violated constraint.
        source: TextError,
    },
    /// HL7 control validation failed.
    #[error("{position}: {source}")]
    Hl7 {
        /// Source location.
        position: Position,
        /// Original control error.
        source: analyte_hl7::Error,
    },
}

/// Host-owned identity, receive time and revision; mapping performs no I/O.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Receipt already retained by the host.
    pub receipt: ReceiptId,
    /// Host receive instant.
    pub received_at: jiff::Timestamp,
    /// Explicit caller-assigned revision, never derived from source status.
    pub revision: Revision,
}

/// A problem requiring host review; contains positions rather than patient data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A required HL7 report field or supported observation state is not established.
    UnqualifiedHl7Report,
    /// Populated source status contradicts an enabled final-only guarantee.
    DepartureConflict,
    /// Version 1 cannot carry the provenance of departure-derived final status.
    DepartureDerivedStatusRequiresContract,

    /// Source value does not fit the configured test expectation.
    ValueKindMismatch {
        /// Required interpretation.
        expected: analyte_config::ValueKind,
    },
    /// Declared component decomposition does not match the source.
    ComponentParts,

    /// Sample selector is missing, empty or explicitly null.
    MissingSample,
    /// No unique source code/system translation exists.
    UnknownTest,
    /// A required identity selector is missing, empty or null.
    MissingIdentity {
        /// Configuration key requiring evidence.
        setting: &'static str,
    },
    /// A source value type cannot be interpreted safely.
    UnsupportedValue,
    /// Formatting/highlighting instructions are retained but never rendered as clean text.
    UnsupportedFormatting,
    /// An escape requires an interpretation agreement that this mapping does not have.
    UnsupportedEscape,
    /// Message processing code is unknown or absent.
    UnknownProcessing,
    /// QC rules produce conflicting classifications.
    ConflictingClassification,
    /// No source observation was present; retain the message for reconciliation.
    NoObservations,
    /// Valid LAW status-only updates require a durable lifecycle representation.
    UnsupportedLawStatusOnly,
    /// Source time could not be interpreted; original text survives.
    InvalidTime,
}

/// Positioned mapping problem; every finding prevents automatic delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source location, with field zero when ownership itself is missing.
    pub position: Position,
    /// Specific issue for operator reconciliation.
    pub problem: Problem,
}

/// One replacement unit and correction evidence in the same order as its parts.
#[derive(Debug)]
pub struct MappedResult {
    /// Common-model content; revision came from the caller.
    pub content: ResultContent,
    /// Whether each source status is an explicitly configured correction code.
    pub corrections: EvidenceList<bool>,
}

/// A source observation that cannot supply a nonempty namespaced test code.
#[derive(Debug)]
pub struct UnmappedObservation {
    /// Original source position.
    pub position: Position,
    /// Original values retained even without a representable test identity.
    pub values: EvidenceList<ResultValue>,
}

/// A note with its exact parent hierarchy, even when that parent has no result.
#[derive(Debug)]
pub struct SourceComment {
    /// Position of the C or NTE record.
    pub position: Position,
    /// Path to the parent; source occurrences disambiguate repeated parents.
    pub parent_path: EvidenceList<SourceNode>,
    /// Original note field, without normalization.
    pub text: SourceText,
}

/// All mapped and unrepresentable observations, together with every discovered issue.
#[derive(Debug, Default)]
pub struct Output {
    /// Typed non-result LAW execution notices, including those in mixed messages.
    pub status_notices: Vec<law::StatusNotice>,
    /// Replacement units, in first-occurrence order.
    pub results: Vec<MappedResult>,
    /// Observations requiring a code/system decision; never silently discarded.
    pub unmapped: Vec<UnmappedObservation>,
    /// Every issue found during mapping.
    pub findings: Vec<Finding>,
    /// All notes once, with their parents; result parts also expose applicable text.
    pub comments: Vec<SourceComment>,
    /// Actual departure applications, separately from enabled configuration.
    pub applied_departures: Vec<AppliedDeparture>,
    evidence_budget: usize,
}

impl Output {
    /// Retains an unknown test as unmapped evidence, with the common parking reason.
    /// No result or test namespace is invented for this observation.
    /// # Errors
    /// Refuses more findings than the mapping evidence bound permits.
    pub fn unmapped_test(
        &mut self,
        position: Position,
        values: EvidenceList<ResultValue>,
    ) -> Result<(), Error> {
        finding(self, position, Problem::UnknownTest)?;
        self.unmapped.push(UnmappedObservation { position, values });
        Ok(())
    }

    /// True when the host must park this receipt; callers must check this before delivery.
    #[must_use]
    pub fn requires_parking(&self) -> bool {
        !self.findings.is_empty()
            || self
                .applied_departures
                .iter()
                .any(|entry| match entry.meaning {
                    DepartureMeaning::FinalReportFromDocumentation
                    | DepartureMeaning::FinalObservationFromDocumentation => true,
                    DepartureMeaning::PackedRecords | DepartureMeaning::WireCompatibility => false,
                })
            || !self.unmapped.is_empty()
            || self.results.iter().any(|r| r.content.requires_parking())
    }
}

fn text(value: &str, position: Position) -> Result<SourceText, Error> {
    SourceText::new(value).map_err(|source| Error::Text { position, source })
}

fn identifier(value: &str, position: Position) -> Result<Identifier, Error> {
    Identifier::new(value).map_err(|source| Error::Text { position, source })
}

fn list<T>(entries: Vec<T>, position: Position) -> Result<EvidenceList<T>, Error> {
    EvidenceList::new(entries).map_err(|_| Error::Limit {
        position,
        resource: "evidence entries",
    })
}

fn finding(output: &mut Output, position: Position, problem: Problem) -> Result<(), Error> {
    if output.findings.len() == 65536 {
        return Err(Error::Limit {
            position,
            resource: "findings",
        });
    }
    output.findings.push(Finding { position, problem });
    Ok(())
}

fn reserve_evidence(
    output: &mut Output,
    source: &inbound::source::Source<'_>,
    index: usize,
) -> Result<(), Error> {
    let position = source.position(index, 0);
    // Charge ancestors and their notes before copying. Repeated parent evidence
    // can expand a small message, so the source byte bound alone is insufficient.
    let mut bytes = 65536_usize;
    for ancestor in source.ancestors(index)? {
        let node = source.nodes.get(ancestor).ok_or(Error::Structure {
            position,
            reason: "evidence owner missing",
        })?;
        for size in std::iter::once(source.raw_size(ancestor))
            .chain(node.comments.iter().map(|i| source.raw_size(*i)))
        {
            bytes = size
                .checked_mul(16)
                .and_then(|size| bytes.checked_add(size))
                .ok_or(Error::Limit {
                    position,
                    resource: "expanded evidence budget",
                })?;
        }
    }
    output.evidence_budget = output
        .evidence_budget
        .checked_add(bytes)
        .filter(|bytes| *bytes <= 67_108_864)
        .ok_or(Error::Limit {
            position,
            resource: "expanded evidence budget",
        })?;
    Ok(())
}

/// Encodes ordered device-sent run evidence without concatenation collisions.
/// Source spelling and order are retained; empty/missing evidence must be handled
/// by the protocol mapper before calling this function.
/// # Errors
/// Rejects a run exceeding the common model's 16384-byte source-text bound,
/// before growing its buffer beyond that bound.
pub fn identity_run(values: &[&str]) -> Result<SourceText, TextError> {
    let mut run = String::new();
    for value in values {
        let prefix = format!("{}:", value.len());
        if run
            .len()
            .checked_add(prefix.len())
            .and_then(|n| n.checked_add(value.len()))
            .is_none_or(|n| n > 16384)
        {
            return Err(TextError::TooLong { limit: 16384 });
        }
        run.push_str(&prefix);
        run.push_str(value);
    }
    SourceText::new(&run)
}
