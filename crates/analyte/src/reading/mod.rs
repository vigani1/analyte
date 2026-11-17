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
    /// Preserves the `Astm` boundary failure.
    #[error(transparent)]
    Astm(#[from] analyte_astm::Error),
    /// Preserves the `Hl7` boundary failure.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
    /// Preserves the `Law` boundary failure.
    #[error(transparent)]
    Law(#[from] analyte_hl7::law::Failure),
    /// Preserves the `Poct` boundary failure.
    #[error(transparent)]
    Poct(#[from] analyte_poct::MessageError),
    /// Preserves the `Vidas` boundary failure.
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// Preserves the `Autolyser` boundary failure.
    #[error(transparent)]
    Autolyser(#[from] analyte_autolyser::syntax::Error),
    /// Preserves the `AstmFrame` boundary failure.
    #[error(transparent)]
    AstmFrame(#[from] analyte_framing::astm::Error),
    /// Preserves the `AstmReceipt` boundary failure.
    #[error(transparent)]
    AstmReceipt(#[from] analyte_framing::astm::ReceiptError),
    /// Preserves the `MllpFrame` boundary failure.
    #[error(transparent)]
    MllpFrame(#[from] analyte_framing::hl7::Error),
    /// Preserves the `MllpReceipt` boundary failure.
    #[error(transparent)]
    MllpReceipt(#[from] analyte_framing::hl7::ReceiptError),
    /// Preserves the `VidasFrame` boundary failure.
    #[error(transparent)]
    VidasFrame(#[from] analyte_framing::vidas::Error),
    /// Preserves the `AutolyserFrame` boundary failure.
    #[error(transparent)]
    AutolyserFrame(#[from] analyte_framing::autolyser::Error),
    /// Preserves the `Encoding` boundary failure.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Preserves the `Mapping` boundary failure.
    #[error(transparent)]
    Mapping(#[from] analyte_mapping::Error),
    /// Preserves the `Evidence` boundary failure.
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
    input: Input,
    raw: &[u8],
    hello: Option<&[u8]>,
) -> Result<Output, Error> {
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Stream(s)) => {
            astm::read(config, input, &payload(config, raw)?, s.syntax())
        }
        Stack::Astm(RecordTransport::Folder(s)) => astm::read(config, input, raw, s.syntax()),
        Stack::Hl7(RecordTransport::Stream(s)) => {
            hl7::read(config, input, &payload(config, raw)?, s.syntax())
        }
        Stack::Hl7(RecordTransport::Folder(s)) => hl7::read(config, input, raw, s.syntax()),
        Stack::Law(s) => law::read(config, input, &payload(config, raw)?, s),
        Stack::Poct(s) => poct::read(config, input, raw, s, hello),
        Stack::Vidas(s) => vidas::read(config, input, raw, s),
        Stack::Autolyser(s) => autolyser::read(config, input, raw, s),
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
    /// VIDAS settings missing.
    #[error("VIDAS settings missing")]
    VidasSettingsMissing,
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
    assert_eq!(
        Error::Context(Context::VidasSettingsMissing).to_string(),
        "VIDAS settings missing"
    );
}
