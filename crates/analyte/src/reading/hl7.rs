//! Reads HL7 syntax with installed policy and maps its exact fields.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: analyte_mapping::Input<'_>,
    payload: &[u8],
    syntax: &analyte_config::Hl7Syntax,
    mappings: &analyte_config::Mappings,
) -> Result<analyte_mapping::Output, Error> {
    let message =
        analyte_hl7::Message::parse(payload, crate::machine::hl7::settings::options(syntax))?;
    Ok(analyte_mapping::hl7::map(
        &crate::machine::hl7::settings::mapping(
            syntax,
            mappings,
            &config.effective().machine().deviations,
        ),
        &message,
        input,
    )?)
}
