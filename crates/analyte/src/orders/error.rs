//! Distinguishes order input, deadlines, storage and settlement failures while retaining their causes.
/// Failures looking up, reserving, rendering or settling machine orders.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// AUTOLYSER order mapping failed.
    #[error(transparent)]
    AutolyserMapping(#[from] analyte_mapping::autolyser::OrderError),
    /// VIDAS order mapping failed.
    #[error(transparent)]
    VidasMapping(#[from] analyte_mapping::vidas::OrderError),
    /// HL7 order mapping failed.
    #[error(transparent)]
    Hl7Mapping(#[from] analyte_mapping::hl7::OrderError),
    /// ASTM order mapping failed.
    #[error(transparent)]
    AstmMapping(#[from] analyte_mapping::astm::OrderError),
    /// Cache collection exceeded the query deadline.
    #[error("operation deadline expired")]
    LookupDeadline,
    /// VIDAS mapped request missing.
    #[error("VIDAS mapped request missing")]
    MappedRequestMissing,
    /// VIDAS order poll deadline overflow.
    #[error("VIDAS order poll deadline overflow")]
    OrderPollDeadlineOverflow,
    /// Unexpected Autolyser insertion outcome.
    #[error("unexpected Autolyser insertion outcome")]
    UnexpectedInsertionOutcome,

    /// Failure injected only by the optional test harness.
    #[error(transparent)]
    TestHook(crate::runtime::test_hooks::Error),
    /// The store refused admission of this order version or lifecycle state.
    #[error(transparent)]
    Refused(#[from] analyte_store::SendRefusal),
    /// ASTM query bytes failed syntax parsing.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    /// The machine link failed while sending order bytes.
    #[error(transparent)]
    Transport(#[from] analyte_transport::Error),
    /// Order bytes could not be framed for MLLP transmission.
    #[error(transparent)]
    Mllp(#[from] analyte_framing::hl7::Error),
    /// The LAW work exchange rejected an event or deadline.
    #[error(transparent)]
    DialogLaw(#[from] analyte_dialog::law::Error),
    /// The HL7 order response exchange rejected an event or deadline.
    #[error(transparent)]
    DialogHl7(#[from] analyte_dialog::hl7::Error),
    /// LAW preparation deadline overflow.
    #[error("LAW preparation deadline overflow")]
    LawPreparationDeadlineOverflow,
    /// HL7 settings missing.
    #[error("HL7 settings missing")]
    Hl7SettingsMissing,
    /// HL7 response deadline overflow.
    #[error("HL7 response deadline overflow")]
    Hl7ResponseDeadlineOverflow,
    /// HL7 order exchange absent before transmission.
    #[error("HL7 order exchange absent before transmission")]
    Hl7ExchangeAbsent,
    /// HL7 send settings missing after order admission.
    #[error("HL7 send settings missing after order admission")]
    Hl7SendSettingsMissing,
    /// LAW query deadline overflow.
    #[error("LAW query deadline overflow")]
    LawQueryDeadlineOverflow,
    /// LAW work layout absent.
    #[error("LAW work layout absent")]
    LawWorkLayoutAbsent,
    /// LAW query failure has no recoverable acknowledgment outcome.
    #[error("LAW query failure has no recoverable acknowledgment outcome")]
    LawQueryFailureHasNoAckOutcome,
    /// ASTM query capability absent.
    #[error("ASTM query capability absent")]
    AstmQueryCapabilityAbsent,
    /// ASTM query deadline overflow.
    #[error("ASTM query deadline overflow")]
    AstmQueryDeadlineOverflow,
    /// ASTM specimen selector contains unsupported escapes.
    #[error("ASTM specimen selector contains unsupported escapes")]
    UnsupportedSpecimenEscapes,
    /// ASTM limits missing.
    #[error("ASTM limits missing")]
    AstmLimitsMissing,
    /// ASTM query response byte limit exceeded.
    #[error("ASTM query response byte limit exceeded")]
    AstmQueryResponseTooLarge,
    /// Original wire mapping failure.
    #[error(transparent)]
    Hl7WireMapping(#[from] analyte_mapping::hl7::WireError),
    /// LAW work mapping failed.
    #[error(transparent)]
    LawMapping(#[from] analyte_mapping::law::WireError),
    /// HL7 orders missing.
    #[error("HL7 orders missing")]
    Hl7OrdersMissing,
    /// HL7 query deadline overflow.
    #[error("HL7 query deadline overflow")]
    Hl7QueryDeadlineOverflow,
    /// HL7 query policy missing.
    #[error("HL7 query policy missing")]
    Hl7QueryPolicyMissing,
    /// LAW container assigning authority outside configured `SampleId` binding.
    #[error("LAW container assigning authority outside configured SampleId binding")]
    ContainerAuthorityMismatch,
    /// LAW malformed query lacks recoverable correlation.
    #[error("LAW malformed query lacks recoverable correlation")]
    QueryCorrelationMissing,
    /// LAW query deadline after reservation.
    #[error("LAW query deadline after reservation")]
    LawQueryDeadlineAfterReservation,
    /// LAW query deadline before reservation.
    #[error("LAW query deadline before reservation")]
    LawQueryDeadlineBeforeReservation,
    /// LAW query deadline during rendering.
    #[error("LAW query deadline during rendering")]
    LawQueryDeadlineDuringRendering,
    /// LAW settings absent.
    #[error("LAW settings absent")]
    LawSettingsAbsent,
    /// unexpected nested LAW followup.
    #[error("unexpected nested LAW followup")]
    UnexpectedNestedLawFollowup,
    /// unsupported LAW query scope.
    #[error("unsupported LAW query scope")]
    UnsupportedLawQueryScope,
    /// Durable order lookup or lifecycle persistence failed.
    #[error(transparent)]
    Store(#[from] analyte_store::OrderError),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// Retained operator evidence could not be read.
    #[error(transparent)]
    Evidence(#[from] analyte_store::OperatorError),
    /// Retaining or interpreting the machine response failed.
    #[error(transparent)]
    Intake(#[from] crate::IntakeError),
    /// An order identity or retained outcome violated model text bounds.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// An app order list violated the model collection bounds.
    #[error(transparent)]
    List(#[from] analyte_model::ListError),
    /// Reading typed outbound configuration failed.
    #[error(transparent)]
    Config(#[from] analyte_config::Error),
    /// HL7 order or response syntax was invalid.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW work or query syntax was invalid.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Error),
    /// LAW query semantics prevented a valid work response.
    #[error(transparent)]
    LawFailure(#[from] analyte_hl7::law::Failure),
    /// App work could not be mapped into the machine order repertoire.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::OrderMappingError),
    /// The cached app order violated the app contract.
    #[error(transparent)]
    Document(#[from] analyte_contract::DocumentError),
    /// An order event timestamp could not be represented.
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    /// Order response text was not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Retained order evidence could not be serialized as JSON.
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
impl crate::cleanup::CleanupFailure for OrderError {
    fn with_effect(original: Self, effect: Self) -> Self {
        Self::Cleanup {
            original: Box::new(original),
            effect: Box::new(effect),
        }
    }
}

impl From<analyte_store::WorkerError> for OrderError {
    fn from(error: analyte_store::WorkerError) -> Self {
        Self::Store(error.into())
    }
}
