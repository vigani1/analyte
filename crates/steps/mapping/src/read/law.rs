//! Reads retained LAW results through the configured HL7 grammar and LAW mapper.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: crate::Input,
    raw: &[u8],
    settings: &analyte_config::LawStack,
) -> Result<crate::Output, Error> {
    let message = analyte_hl7::law::parse_message(raw, settings.options())?;
    let results = analyte_hl7::law::receive_results(&message)?;
    Ok(crate::map_law(&results, config, input)?)
}
