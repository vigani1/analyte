//! Verifies Autolyser receipt checksums before syntax and mapping.
use super::Error;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: crate::Input,
    raw: &[u8],
    settings: &analyte_config::AutolyserStack,
) -> Result<crate::Output, Error> {
    let body = analyte_framing::autolyser::verified(raw)?;
    let report = analyte_autolyser::syntax::Report::parse(body, settings.policy().controls)?;
    Ok(crate::map_autolyser(
        config,
        input.receipt,
        &report,
        input.received_at,
    )?)
}
