//! Reads retained receipts through the same framing, syntax and mapping policies as live intake.
use analyte_config::{RecordTransport, Stack, Validated};
use analyte_mapping::{Input, Output};
mod astm;
mod autolyser;
mod envelope;
mod hl7;
mod law;
mod poct;
mod vidas;
pub(crate) use envelope::payload;
pub(crate) use poct::limits as poct_limits;
pub(crate) use vidas::framing as vidas_config;

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

/// Reads retained original bytes without I/O, orders, acknowledgments or storage mutations.
/// `hello` is the original durable POCT connection Hello, when available.
/// # Errors
/// Returns the original layer failure; never supplies a partial mapping on failure.
pub(crate) fn read(
    config: &Validated,
    input: Input<'_>,
    raw: &[u8],
    hello: Option<&[u8]>,
) -> Result<Output, Error> {
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Stream(s)) => astm::read(
            input,
            &payload(config, raw)?,
            s.settings().syntax(),
            s.settings().codes(),
        ),
        Stack::Astm(RecordTransport::Folder(s)) => {
            astm::read(input, raw, s.settings().syntax(), s.settings().codes())
        }
        Stack::Hl7(RecordTransport::Stream(s)) => hl7::read(
            config,
            input,
            &payload(config, raw)?,
            s.settings().syntax(),
            s.settings().codes(),
        ),
        Stack::Hl7(RecordTransport::Folder(s)) => hl7::read(
            config,
            input,
            raw,
            s.settings().syntax(),
            s.settings().codes(),
        ),
        Stack::Law(s) => law::read(input, &payload(config, raw)?, s),
        Stack::Poct(s) => poct::read(input, raw, s, hello),
        Stack::Vidas(s) => vidas::read(input, raw, s),
        Stack::Autolyser(s) => autolyser::read(input, raw, s),
    }
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
