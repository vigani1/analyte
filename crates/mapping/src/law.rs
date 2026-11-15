//! LAW result and outgoing AWOS mapping; protocol validation stays in analyte-hl7.
use crate::inbound::{grouping, source};
use crate::{Error, Input, Output, is_law};
use analyte_config::Validated;
mod orders;
pub use orders::{Command, address, map_order};
/// Maps atomically validated LAB-29 through the shared field mapper and translations.
/// AWOS identity takes precedence; unsolicited work needs an explicit analyzer identity.
/// # Errors
/// Rejects mismatched configuration, unsupported mapping content or resource bounds.
pub fn map(
    results: &analyte_hl7::law::Results<'_>,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    if !is_law(config) {
        return Err(Error::Syntax);
    }
    crate::hl7::validate_message(results.message(), config)?;
    let mut source = source::law(results)?;
    source.attach_comments()?;
    grouping::message(&source, config, input, Some(results))
}
