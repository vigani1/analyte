//! Reads HL7 syntax with installed policy and maps its exact fields.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: analyte_mapping::Input<'_>,
    payload: &[u8],
    syntax: &analyte_config::Hl7Syntax,
    mappings: &analyte_config::Mappings,
) -> Result<analyte_mapping::Output, Error> {
    let message = analyte_hl7::Message::parse(payload, syntax.options())?;
    Ok(analyte_mapping::hl7::map(
        &analyte_mapping::hl7::Settings {
            syntax,
            mappings,
            declarations: &config.effective().machine().deviations,
        },
        &message,
        input,
    )?)
}
