//! Reads HL7 syntax with installed policy and maps its exact fields.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: analyte_mapping::Input,
    payload: &[u8],
    syntax: &analyte_config::Hl7Syntax,
) -> Result<analyte_mapping::Output, Error> {
    let message = analyte_hl7::Message::parse(payload, syntax.options())?;
    Ok(analyte_mapping::map_hl7(&message, config, input)?)
}
