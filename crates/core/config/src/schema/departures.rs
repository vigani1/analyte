//! Defines the closed machine departure vocabulary and its bounded evidence scope.
use super::{Edition, EvidenceLevel, Identifier};
use serde::{Deserialize, Serialize};

/// Exact traffic direction; inbound tolerance never authorizes outbound changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepartureDirection {
    /// Analyzer uploads.
    MachineToHost,
    /// Host transmissions.
    HostToMachine,
}

/// Closed traffic selection, never a free field path or pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepartureMessage {
    /// Generic patient result upload only.
    OruR01Patient,
    /// ASTM LIS02 result upload.
    AstmResults,
    /// ASTM link control.
    AstmLink,
}

/// Selected protocol edition; HL7 and ASTM cannot be interchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protocol", deny_unknown_fields, rename_all = "snake_case")]
pub enum DepartureEdition {
    /// Generic HL7 edition.
    Hl7 {
        /// Exact supported edition.
        edition: Edition,
    },
    /// LIS01-A2 / LIS02-A2.
    Astm,
}

/// Installed applicability; all text is bounded and no wildcard is interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureScope {
    /// Confirmed analyzer model.
    pub model: Identifier,
    /// Exact firmware, or explicit unknown with vendor applicability evidence.
    pub firmware: Identifier,
    /// Installed protocol mode.
    pub mode: Identifier,
    /// Selected protocol and edition.
    pub edition: DepartureEdition,
    /// Selected traffic direction.
    pub direction: DepartureDirection,
    /// Selected message kind.
    pub message: DepartureMessage,
}

/// Exact documented guarantee; a reviewer verifies the cited document's meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepartureGuarantee {
    /// Vendor explicitly guarantees final reports when OBR-25 is absent.
    AbsentObr25Final,
    /// Vendor explicitly guarantees final observations when OBX-11 is absent.
    AbsentObx11Final,
    /// Vendor explicitly documents packed inbound records.
    PackedRecords,
    /// Vendor documents: Empty H.12 in a documented upload is treated as production.
    AbsentProcessingIdProduction,
    /// Vendor documents: One repeated ENQ before any frame can recover a lost establishment ACK.
    RepeatEnq,
    /// Existing compatibility option is explicitly documented.
    Compatibility,
}

/// Evidence references are operator-resolvable IDs; protected manuals stay outside Git.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields, rename_all = "snake_case")]
pub enum DepartureReference {
    /// A manual or explicit vendor clarification, never fetched by the loader.
    Manual {
        /// Stable operator-facing protected-reference ID, not a filesystem path.
        reference: Identifier,
        /// Document title.
        title: Identifier,
        /// Document revision.
        revision: Identifier,
        /// Exact source section.
        section: Identifier,
        /// Meaning established by this source, reviewed by a human.
        guarantee: DepartureGuarantee,
        /// Explicit vendor coverage for unknown installed firmware.
        covers_unknown_firmware: bool,
        /// Contradictory vendor statements block activation.
        contradicts: bool,
    },
    /// A local ADR 0009 recording, digest and event range.
    Recording {
        /// Relative local path, resolved against the config layer's directory.
        file: Identifier,
        /// SHA-256 of the exact recording bytes.
        sha256: Identifier,
        /// One-based first event of the exercised exchange.
        first_event: u32,
        /// One-based last event of the exercised exchange.
        last_event: u32,
    },
}

/// Declaration data validated once before a runtime snapshot can be activated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureDeclaration {
    /// Exact installed scope.
    pub scope: DepartureScope,
    /// Capability-specific evidence level, never promoted by synthetic tests.
    pub level: EvidenceLevel,
    /// One through sixteen qualifying local references.
    pub evidence: Vec<DepartureReference>,
}

/// Exhaustive named rules; no arbitrary replacement, field path or validation bypass.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "rule",
    content = "declaration",
    deny_unknown_fields,
    rename_all = "snake_case"
)]
pub enum MachineDeparture {
    /// Missing OBR-25 has documented final-report meaning.
    Hl7AbsentObr25MeansFinal(DepartureDeclaration),
    /// Missing OBX-11 has documented final-observation meaning.
    Hl7AbsentObx11MeansFinal(DepartureDeclaration),
    /// Several CR-ended inbound LIS02 records share a verified frame.
    AstmPackedRecordsInFrame(DepartureDeclaration),
    /// Empty H.12 in a documented upload is treated as production.
    AstmAbsentH12MeansProduction(DepartureDeclaration),
    /// One repeated ENQ before any frame can recover a lost establishment ACK.
    AstmRepeatEnqAfterLostAck(DepartureDeclaration),
    /// Accept the later UTF-8 designation in HL7 2.3.1.
    Hl7Utf8InV231(DepartureDeclaration),
    /// Use CRLF segment endings.
    Hl7CrLfTerminators(DepartureDeclaration),
    /// Use LF segment endings.
    Hl7LfTerminators(DepartureDeclaration),
    /// Preserve unsupported escapes without granting semantic identity.
    Hl7PreserveUnsupportedEscapes(DepartureDeclaration),
    /// Use the explicitly selected bounded frame limit.
    AstmFrameLimit(DepartureDeclaration),
    /// Use the explicitly selected bounded receive deadline.
    AstmReceiveTimeout(DepartureDeclaration),
    /// Use the explicitly selected bounded send deadline.
    AstmSendTimeout(DepartureDeclaration),
    /// Give the host contention priority.
    AstmHostContentionPriority(DepartureDeclaration),
    /// Accept lowercase hex with a correct checksum.
    AstmLowercaseChecksumHex(DepartureDeclaration),
}

impl MachineDeparture {
    /// Encodes the exact declaration in the existing TOML format for protected interpretation evidence.
    /// # Errors
    /// Refuses serialization failures or a snapshot exceeding the source-text evidence bound.
    pub fn evidence_snapshot(&self) -> Result<analyte_model::SourceText, crate::Error> {
        let encoded = toml::to_string(self)?;
        analyte_model::SourceText::new(&encoded).map_err(|source| crate::Error::Departure {
            path: "effective config".into(),
            rule: self.tag().into(),
            reason: super::DepartureRefusal::SnapshotLimit,
            source: Some(Box::new(source)),
        })
    }

    /// Describes the fixed substitute meaning for operator review, never for execution.
    #[must_use]
    pub fn meaning(&self) -> &'static str {
        match self {
            Self::Hl7AbsentObr25MeansFinal(_) => {
                "Only absent OBR-25 means a complete final report; observation statuses are independent."
            }
            Self::Hl7AbsentObx11MeansFinal(_) => {
                "Only absent OBX-11 means a final observation; report completion is independent."
            }
            Self::AstmPackedRecordsInFrame(_) => {
                "Verified inbound frames may contain several CR-ended records; semantic content is unchanged."
            }
            Self::AstmAbsentH12MeansProduction(_) => {
                "Empty H.12 in a documented upload is treated as production."
            }
            Self::AstmRepeatEnqAfterLostAck(_) => {
                "One repeated ENQ before any frame can recover a lost establishment ACK."
            }
            Self::Hl7Utf8InV231(_) => "The later UTF-8 designation is accepted in HL7 2.3.1.",
            Self::Hl7CrLfTerminators(_) => "CRLF separates source segments.",
            Self::Hl7LfTerminators(_) => "LF separates source segments.",
            Self::Hl7PreserveUnsupportedEscapes(_) => {
                "Unsupported escapes remain original evidence and cannot provide identity."
            }
            Self::AstmFrameLimit(_) => "The declared bounded nondefault frame limit is selected.",
            Self::AstmReceiveTimeout(_) => "The declared bounded receive deadline is selected.",
            Self::AstmSendTimeout(_) => "The declared bounded send deadline is selected.",
            Self::AstmHostContentionPriority(_) => {
                "The host wins simultaneous establishment contention."
            }
            Self::AstmLowercaseChecksumHex(_) => {
                "Lowercase checksum digits are allowed only when the checksum verifies."
            }
        }
    }

    /// Returns the declaration without interpreting descriptive text.
    #[must_use]
    pub fn declaration(&self) -> &DepartureDeclaration {
        match self {
            Self::Hl7AbsentObr25MeansFinal(value)
            | Self::Hl7AbsentObx11MeansFinal(value)
            | Self::AstmAbsentH12MeansProduction(value)
            | Self::AstmRepeatEnqAfterLostAck(value)
            | Self::AstmPackedRecordsInFrame(value)
            | Self::Hl7Utf8InV231(value)
            | Self::Hl7CrLfTerminators(value)
            | Self::Hl7LfTerminators(value)
            | Self::Hl7PreserveUnsupportedEscapes(value)
            | Self::AstmFrameLimit(value)
            | Self::AstmReceiveTimeout(value)
            | Self::AstmSendTimeout(value)
            | Self::AstmHostContentionPriority(value)
            | Self::AstmLowercaseChecksumHex(value) => value,
        }
    }

    /// Returns the stable serialized rule tag used in provenance.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Hl7AbsentObr25MeansFinal(_) => "hl7_absent_obr25_means_final",
            Self::Hl7AbsentObx11MeansFinal(_) => "hl7_absent_obx11_means_final",
            Self::AstmPackedRecordsInFrame(_) => "astm_packed_records_in_frame",
            Self::AstmAbsentH12MeansProduction(_) => "astm_absent_h12_means_production",
            Self::AstmRepeatEnqAfterLostAck(_) => "astm_repeat_enq_after_lost_ack",
            Self::Hl7Utf8InV231(_) => "hl7_utf8_in_v231",
            Self::Hl7CrLfTerminators(_) => "hl7_cr_lf_terminators",
            Self::Hl7LfTerminators(_) => "hl7_lf_terminators",
            Self::Hl7PreserveUnsupportedEscapes(_) => "hl7_preserve_unsupported_escapes",
            Self::AstmFrameLimit(_) => "astm_frame_limit",
            Self::AstmReceiveTimeout(_) => "astm_receive_timeout",
            Self::AstmSendTimeout(_) => "astm_send_timeout",
            Self::AstmHostContentionPriority(_) => "astm_host_contention_priority",
            Self::AstmLowercaseChecksumHex(_) => "astm_lowercase_checksum_hex",
        }
    }
}
