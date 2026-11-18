//! Reads retained receipts per protocol and names each reading layer's failures.
//! `machine::protocols` selects the reader from the installed stack.
pub(crate) mod astm;
pub(crate) mod autolyser;
pub(crate) mod hl7;
pub(crate) mod law;
pub(crate) mod poct;
pub(crate) mod vidas;

/// Specific failures from each owned reading layer; callers retain and park the receipt.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// LAW result mapping refused retained evidence.
    #[error(transparent)]
    LawMapping(#[from] analyte_mapping::law::Error),
    /// ASTM record syntax was rejected.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    /// HL7 segment syntax was rejected.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// LAW profile requirements were violated.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Failure),
    /// POCT message grammar was rejected.
    #[error(transparent)]
    Poct(#[from] analyte_poct::MessageError),
    /// VIDAS record syntax was rejected.
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// Autolyser record syntax was rejected.
    #[error(transparent)]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    /// ASTM frame integrity or size checks failed.
    #[error(transparent)]
    AstmFrame(#[from] analyte_framing::astm::Error),
    /// Retained ASTM frames could not form a complete message.
    #[error(transparent)]
    AstmReceipt(#[from] analyte_framing::astm::ReceiptError),
    /// MLLP envelope framing failed.
    #[error(transparent)]
    MllpFrame(#[from] analyte_framing::hl7::Error),
    /// Retained MLLP bytes could not form a complete message.
    #[error(transparent)]
    MllpReceipt(#[from] analyte_framing::hl7::ReceiptError),
    /// VIDAS envelope integrity checks failed.
    #[error(transparent)]
    VidasFrame(#[from] analyte_framing::vidas::Error),
    /// Autolyser envelope integrity checks failed.
    #[error(transparent)]
    AutolyserFrame(#[from] analyte_framing::autolyser::Error),
    /// Receipt text is not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Configured field extraction failed.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::Error),
    /// Result identity or value evidence could not be constructed.
    #[error(transparent)]
    Evidence(#[from] analyte_mapping::ResultMappingError),
    /// Required receipt or configuration context is unavailable.
    #[error("{0}")]
    Context(Context),
}

/// Specific missing or ambiguous retained-receipt context.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Context {
    /// ASTM transfer does not contain exactly one message; operator review required.
    #[error("ASTM transfer does not contain exactly one message; operator review required")]
    AstmMessageCount,
    /// ASTM message missing.
    #[error("ASTM message missing")]
    AstmMessageMissing,
}
#[test]
fn context_spellings_are_pinned() {
    assert_eq!(
        Error::Context(Context::AstmMessageCount).to_string(),
        "ASTM transfer does not contain exactly one message; operator review required"
    );
    assert_eq!(
        Error::Context(Context::AstmMessageMissing).to_string(),
        "ASTM message missing"
    );
}

impl Error {
    pub(crate) fn hl7_mapping_error(
        &self,
    ) -> Option<Result<&analyte_hl7::Error, &analyte_mapping::Error>> {
        use Error as Read;
        match self {
            Read::Hl7(error) => Some(Ok(error)),
            Read::Mapping(error) => Some(Err(error)),
            Read::LawMapping(error) => match error {
                analyte_mapping::law::Error::Mapping(error) => Some(Err(error)),
                analyte_mapping::law::Error::MissingHeader
                | analyte_mapping::law::Error::NoticeOrder
                | analyte_mapping::law::Error::NoticePosition => None,
            },
            Read::Astm(_)
            | Read::Law(_)
            | Read::Poct(_)
            | Read::Vidas(_)
            | Read::Autolyser(_)
            | Read::AstmFrame(_)
            | Read::AstmReceipt(_)
            | Read::MllpFrame(_)
            | Read::MllpReceipt(_)
            | Read::VidasFrame(_)
            | Read::AutolyserFrame(_)
            | Read::Encoding(_)
            | Read::Evidence(_)
            | Read::Context(_) => None,
        }
    }
}
