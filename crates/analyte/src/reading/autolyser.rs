//! Verifies Autolyser receipt checksums before syntax and mapping.
use super::Error;
pub(super) fn read(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::AutolyserStack,
) -> Result<analyte_mapping::Output, Error> {
    let body = analyte_framing::autolyser::decode(raw)?;
    let report = analyte_autolyser::syntax::Report::parse(body, settings.policy().controls)?;
    Ok(analyte_mapping::autolyser::map(settings, &report, input)?)
}
