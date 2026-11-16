//! Distinguishes order input, deadlines, storage and settlement failures while retaining their causes.
/// Failures looking up, reserving, rendering or settling machine orders.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// Failure injected only by the optional test harness.
    #[cfg(feature = "test-hooks")]
    #[error(transparent)]
    TestHook(crate::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Refused(#[from] analyte_store::SendRefusal),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Transport(#[from] analyte_transport::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Mllp(#[from] analyte_framing::hl7::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    DialogLaw(#[from] analyte_dialog::law::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    DialogHl7(#[from] analyte_dialog::hl7::Error),
    /// A required order state or deadline is unavailable.
    #[error("LAW preparation deadline overflow")]
    LawPreparationDeadlineOverflow,
    /// A required order state or deadline is unavailable.
    #[error("HL7 settings missing")]
    Hl7SettingsMissing,
    /// A required order state or deadline is unavailable.
    #[error("HL7 response deadline overflow")]
    Hl7ResponseDeadlineOverflow,
    /// A required order state or deadline is unavailable.
    #[error("exchange absent")]
    ExchangeAbsent,
    /// A required order state or deadline is unavailable.
    #[error("send settings")]
    SendSettings,
    /// Required order state, input or deadline is unavailable.
    #[error("LAW query deadline overflow")]
    LawQueryDeadlineOverflow,
    /// Required order state, input or deadline is unavailable.
    #[error("LAW work layout absent")]
    LawWorkLayoutAbsent,
    /// Required order state, input or deadline is unavailable.
    #[error("query failure has no ACK outcome")]
    QueryFailureHasNoAckOutcome,
    /// Required order state, input or deadline is unavailable.
    #[error("query capability absent")]
    QueryCapabilityAbsent,
    /// Required order state, input or deadline is unavailable.
    #[error("query deadline overflow")]
    QueryDeadlineOverflow,
    /// Required order state, input or deadline is unavailable.
    #[error("ASTM specimen selector contains unsupported escapes")]
    AstmSpecimenSelectorContainsUnsupportedEscapes,
    /// Required order state, input or deadline is unavailable.
    #[error("ASTM limits missing")]
    AstmLimitsMissing,
    /// Required order state, input or deadline is unavailable.
    #[error("query response byte limit exceeded")]
    QueryResponseByteLimitExceeded,
    /// Original wire mapping failure.
    #[error(transparent)]
    WireMapping(#[from] analyte_mapping::WireMappingError),
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("HL7 orders missing")]
    Hl7OrdersMissing,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("HL7 query deadline overflow")]
    Hl7QueryDeadlineOverflow,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("HL7 query policy missing")]
    Hl7QueryPolicyMissing,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW container assigning authority outside configured SampleId binding")]
    LawContainerAssigningAuthorityOutsideConfiguredSampleidBinding,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW malformed query lacks recoverable correlation")]
    LawMalformedQueryLacksRecoverableCorrelation,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW query deadline after reservation")]
    LawQueryDeadlineAfterReservation,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW query deadline before reservation")]
    LawQueryDeadlineBeforeReservation,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW query deadline during rendering")]
    LawQueryDeadlineDuringRendering,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("LAW settings absent")]
    LawSettingsAbsent,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("unexpected nested LAW followup")]
    UnexpectedNestedLawFollowup,
    /// The order operation cannot proceed under its current input, state or deadline.
    #[error("unsupported LAW query scope")]
    UnsupportedLawQueryScope,
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Store(#[from] analyte_store::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    LawFailure(#[from] analyte_hl7::law::Failure),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::OrderMappingError),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Document(#[from] analyte_contract::DocumentError),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Sink(#[from] analyte_sink::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Preserves the original boundary failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Retains both the triggering failure and a later independent settlement failure.
    #[error("{original}; dialog effect failed: {effect}")]
    Cleanup {
        /// Triggering operation failure.
        #[source]
        original: Box<Self>,
        /// Independent failure while settling the operation.
        effect: Box<Self>,
    },
}
impl crate::machine::custody::CleanupFailure for OrderError {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}
