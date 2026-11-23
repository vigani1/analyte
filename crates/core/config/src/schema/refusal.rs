//! Names specific reasons why a machine departure cannot be activated.

/// Distinct load-time refusals from ADR 0015; never a generic tolerance failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DepartureRefusal {
    /// The exact declaration cannot fit the bounded interpretation evidence field.
    #[error("declaration snapshot exceeds 16384 UTF-8 bytes")]
    SnapshotLimit,
    /// Legacy name/key/reason declarations require explicit migration with scope and qualifying evidence.
    #[error(
        "legacy name/key/reason declarations require explicit migration with scope and qualifying evidence"
    )]
    LegacyDeclaration,
    /// At most twelve departure declarations are allowed.
    #[error("at most twelve departure declarations are allowed")]
    TooManyDeclarations,
    /// Duplicate rule declaration.
    #[error("duplicate rule declaration")]
    DuplicateRule,
    /// Conflicting departure variants.
    #[error("conflicting departure variants")]
    ConflictingVariants,
    /// Declare the installed model, firmware, mode, edition, direction and message kind.
    #[error("declare the installed model, firmware, mode, edition, direction and message kind")]
    MissingInstalledScope,
    /// Declaration does not match installed applicability.
    #[error("declaration does not match installed applicability")]
    ScopeMismatch,
    /// Wildcard or unspecified model, mode or firmware applicability is unsupported.
    #[error("wildcard or unspecified model, mode or firmware applicability is unsupported")]
    UnsupportedApplicability,
    /// Departure does not belong to the selected protocol.
    #[error("departure does not belong to the selected protocol")]
    ProtocolMismatch,
    /// Departure does not belong to the selected edition.
    #[error("departure does not belong to the selected edition")]
    EditionMismatch,
    /// Departure does not belong to the selected direction.
    #[error("departure does not belong to the selected direction")]
    DirectionMismatch,
    /// Departure does not belong to the selected message kind.
    #[error("departure does not belong to the selected message kind")]
    MessageMismatch,
    /// Declaration disagrees with the selected compatibility setting.
    #[error("declaration disagrees with the selected compatibility setting")]
    InactiveSetting,
    /// Selected nonstandard setting requires an evidenced declaration.
    #[error("selected nonstandard setting requires an evidenced declaration")]
    MissingDeclaration,
    /// At least one qualifying evidence reference is required.
    #[error("at least one qualifying evidence reference is required")]
    EmptyEvidence,
    /// At most sixteen evidence references are allowed.
    #[error("at most sixteen evidence references are allowed")]
    EvidenceLimit,
    /// Required rule-specific manual or vendor guarantee is missing.
    #[error("required rule-specific manual or vendor guarantee is missing")]
    MissingRuleEvidence,
    /// Unknown firmware requires explicit vendor coverage.
    #[error("unknown firmware requires explicit vendor coverage")]
    UnknownFirmware,
    /// Contradictory vendor statements prevent activation.
    #[error("contradictory vendor statements prevent activation")]
    ContradictoryEvidence,
    /// Manual title, revision, section and protected reference ID must be resolvable identifiers.
    #[error(
        "manual title, revision, section and protected reference ID must be resolvable identifiers"
    )]
    InvalidManualReference,
    /// Recording reference must be a bounded relative local .rec.jsonl path without traversal.
    #[error(
        "recording reference must be a bounded relative local .rec.jsonl path without traversal"
    )]
    InvalidLocalReference,
    /// Recording reference could not be resolved as a local regular file.
    #[error("recording reference could not be resolved as a local regular file")]
    UnresolvedRecording,
    /// Local recording could not be read.
    #[error("local recording could not be read")]
    RecordingRead,
    /// Recording exceeds the bounded file, line, event or payload limits.
    #[error("recording exceeds the bounded file, line, event or payload limits")]
    RecordingLimit,
    /// Recording SHA-256 does not match.
    #[error("recording SHA-256 does not match")]
    DigestMismatch,
    /// Reference is not a valid supported ADR 0009 recording.
    #[error("reference is not a valid supported ADR 0009 recording")]
    MalformedRecording,
    /// Recorded exchange is missing or does not exercise the declared direction.
    #[error("recorded exchange is missing or does not exercise the declared direction")]
    ExchangeMismatch,
    /// Recording model, firmware or protocol does not match the declaration.
    #[error("recording model, firmware or protocol does not match the declaration")]
    RecordingScopeMismatch,
    /// Claimed level lacks corresponding exercised capability evidence.
    #[error("claimed level lacks corresponding exercised capability evidence")]
    LevelEvidence,
    /// A record-only connection keeps its shipped capture profile; a model's
    /// record file may declare only packed ASTM records in one frame (ADR 0034).
    #[error(
        "a record file may declare only astm_packed_records_in_frame; every other link rule is the shipped capture profile's"
    )]
    RecordOnlyRule,
    /// A record file's departure names a mode its `[analyzer_mode]` does not offer.
    #[error("the declared mode is not one of the values the record file's [analyzer_mode] offers")]
    ModeNotOffered,
}
