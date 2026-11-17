//! Verifies Autolyser receipt checksums before syntax and mapping.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: analyte_mapping::Input,
    raw: &[u8],
    settings: &analyte_config::AutolyserStack,
) -> Result<analyte_mapping::Output, Error> {
    let body = analyte_framing::autolyser::verified(raw)?;
    let report = analyte_autolyser::syntax::Report::parse(body, settings.policy().controls)?;
    Ok(analyte_mapping::map_autolyser(
        config,
        input.receipt,
        &report,
        input.received_at,
    )?)
}
