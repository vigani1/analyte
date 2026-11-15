//! Pure, bounded mapping of documented LIS02 and HL7 laboratory results.
//! The caller retains the receipt and parks any output with findings.
#![forbid(unsafe_code)]

mod identity;
mod map;
mod orders;
mod result;
mod source;
mod time;
mod value;
pub use orders::*;

use analyte_config::{SyntaxKind, Validated};
use analyte_model::*;

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
#[error("{0:?} is not a field-position syntax")]
pub struct UnsupportedFieldSyntax(pub SyntaxKind);

impl TryFrom<SyntaxKind> for FieldSyntax {
    type Error = UnsupportedFieldSyntax;

    fn try_from(syntax: SyntaxKind) -> Result<Self, Self::Error> {
        match syntax {
            SyntaxKind::AstmLis02 => Ok(Self::AstmLis02),
            SyntaxKind::Hl7V2 => Ok(Self::Hl7V2),
            SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser => {
                Err(UnsupportedFieldSyntax(syntax))
            }
        }
    }
}

fn field_mappings(config: &Validated) -> Result<&analyte_config::Mappings, Error> {
    config
        .effective()
        .stack()
        .field_mappings()
        .ok_or(Error::Syntax)
}

fn is_law(config: &Validated) -> bool {
    use analyte_config::Stack;
    match config.effective().stack() {
        Stack::Law(_) => true,
        Stack::Astm(_)
        | Stack::FileAstm(_)
        | Stack::Hl7(_)
        | Stack::FileHl7(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_) => false,
    }
}

fn astm_syntax(config: &Validated) -> Result<&analyte_config::AstmSyntax, Error> {
    use analyte_config::Stack;
    match config.effective().stack() {
        Stack::Astm(s) => Ok(&s.syntax),
        Stack::FileAstm(s) => Ok(&s.syntax),
        Stack::Hl7(_)
        | Stack::FileHl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_) => Err(Error::Syntax),
    }
}

fn hl7_edition(config: &Validated) -> Result<analyte_config::Edition, Error> {
    use analyte_config::Stack;
    match config.effective().stack() {
        Stack::Hl7(s) => Ok(s.syntax.edition),
        Stack::FileHl7(s) => Ok(s.syntax.edition),
        Stack::Law(_) => Ok(analyte_config::Edition::V251),
        Stack::Astm(_)
        | Stack::FileAstm(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_) => Err(Error::Syntax),
    }
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
    /// The validated config belongs to another syntax family.
    #[error("configuration syntax does not match the parsed message")]
    Syntax,
    /// A bound was reached before producing a partial success.
    #[error("{position:?}: {resource} limit exceeded")]
    Limit {
        /// Source location.
        position: Position,
        /// Exhausted bound.
        resource: &'static str,
    },
    /// Source structure cannot safely establish field ownership.
    #[error("{position:?}: {reason}")]
    Structure {
        /// Source location.
        position: Position,
        /// Specific structural failure.
        reason: &'static str,
    },
    /// Bytes cannot be represented as source text in the declared encoding.
    #[error("{position:?}: invalid source encoding")]
    Encoding {
        /// Source location.
        position: Position,
    },
    /// A common-model text constraint failed.
    #[error("{position:?}: {source}")]
    Text {
        /// Source location.
        position: Position,
        /// Exact violated constraint.
        source: TextError,
    },
    /// HL7 control validation failed.
    #[error("{position:?}: {source}")]
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
    /// Message processing code is unknown or absent.
    UnknownProcessing,
    /// QC rules produce conflicting classifications.
    ConflictingClassification,
    /// No source observation was present; retain the message for reconciliation.
    NoObservations,
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
    /// Replacement units, in first-occurrence order.
    pub results: Vec<MappedResult>,
    /// Observations requiring a code/system decision; never silently discarded.
    pub unmapped: Vec<UnmappedObservation>,
    /// Every issue found during mapping.
    pub findings: Vec<Finding>,
    /// All notes once, with their parents; result parts also expose applicable text.
    pub comments: Vec<SourceComment>,
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
            || !self.unmapped.is_empty()
            || self.results.iter().any(|r| r.content.requires_parking())
    }
}

/// Maps one parsed LIS02 message, retaining hierarchy and original value text.
/// # Errors
/// Rejects a mismatched config, invalid ownership, encoding or model resource bounds.
pub fn map_astm(
    message: &analyte_astm::Message,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    let syntax = astm_syntax(config)?;
    let settings = config.effective().stack().limits();
    if message.records().len() > settings.records {
        return Err(Error::Limit {
            position: Position {
                record: 0,
                field: 0,
            },
            resource: "records",
        });
    }
    let mut bytes = 0_usize;
    for (index, record) in message.records().iter().enumerate() {
        bytes = bytes
            .checked_add(record.raw().len())
            .and_then(|b| b.checked_add(1))
            .ok_or(Error::Limit {
                position: Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                resource: "message bytes",
            })?;
        if bytes > settings.bytes || record.raw().len() > settings.record_bytes {
            return Err(Error::Limit {
                position: Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                resource: "message or record bytes",
            });
        }
    }
    let mut source = source::astm(message);
    source.charset = match syntax.encoding {
        analyte_config::Encoding::Ascii => analyte_hl7::Charset::Ascii,
        analyte_config::Encoding::Latin1 => analyte_hl7::Charset::Latin1,
        analyte_config::Encoding::Utf8 => analyte_hl7::Charset::Utf8,
    };
    source.attach_comments()?;
    map::message(&source, config, input, None)
}

/// Maps ORU R01 or OUL R22 (2.5.1 only), with group-relative selectors.
/// # Errors
/// Rejects unsupported/misplaced segments rather than guessing group ownership.
pub fn map_hl7(
    message: &analyte_hl7::Message,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    if is_law(config) {
        return Err(Error::Syntax);
    }
    check_hl7(message, config)?;
    map_hl7_checked(message, config, input)
}

/// Maps atomically validated LAB-29 through the shared field mapper and translations.
/// AWOS identity takes precedence; unsolicited work needs an explicit analyzer identity.
/// # Errors
/// Rejects mismatched configuration, unsupported mapping content or resource bounds.
pub fn map_law(
    results: &analyte_hl7::law::Results<'_>,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    if !is_law(config) {
        return Err(Error::Syntax);
    }
    check_hl7(results.message(), config)?;
    let mut source = source::law(results)?;
    source.attach_comments()?;
    map::message(&source, config, input, Some(results))
}

fn check_hl7(message: &analyte_hl7::Message, config: &Validated) -> Result<(), Error> {
    if config
        .effective()
        .hl7_options()
        .is_none_or(|o| o.charset != message.charset())
    {
        return Err(Error::Syntax);
    }
    let settings = config.effective().stack().limits();
    if message.raw().len() > settings.bytes || message.segments().count() > settings.records {
        return Err(Error::Limit {
            position: Position {
                record: 0,
                field: 0,
            },
            resource: "message bytes or segments",
        });
    }
    Ok(())
}

fn map_hl7_checked(
    message: &analyte_hl7::Message,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    let edition = hl7_edition(config)?;
    let mut source = source::hl7(message)?;
    let version = analyte_hl7::Msh::new(message)
        .map_err(|source| Error::Hl7 {
            position: Position {
                record: 1,
                field: 12,
            },
            source,
        })?
        .version();
    if !matches!(
        (edition, version),
        (analyte_config::Edition::V231, analyte_hl7::Version::V231)
            | (analyte_config::Edition::V251, analyte_hl7::Version::V251)
    ) {
        return Err(Error::Structure {
            position: Position {
                record: 1,
                field: 12,
            },
            reason: "message edition disagrees with validated configuration",
        });
    }
    source.attach_comments()?;
    map::message(&source, config, input, None)
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

fn present(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty() && s != "\"\"")
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
    source: &source::Source<'_>,
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
