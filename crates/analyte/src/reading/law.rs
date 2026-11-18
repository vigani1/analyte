//! Reads retained LAW results through the configured HL7 grammar and LAW mapper.
use super::Error;
pub(crate) fn read(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::LawStack,
) -> Result<analyte_mapping::Output, Error> {
    let message =
        analyte_hl7::law::parse_message(raw, crate::machine::law::settings::options(settings))?;
    let results = analyte_hl7::law::receive_results(&message)?;
    Ok(analyte_mapping::law::map(settings, &results, input)?)
}
