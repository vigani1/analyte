//! Reads VIDAS packets using the installed framing and sample identity policy.
use super::Error;
pub(crate) fn read(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::VidasStack,
) -> Result<analyte_mapping::Output, Error> {
    let packet =
        analyte_framing::vidas::decode(raw, crate::machine::vidas::settings::framing(settings)?)?;
    let limit = crate::machine::vidas::settings::sample_limit(settings);
    let message = analyte_vidas::syntax::Message::parse(std::str::from_utf8(&packet.body)?, limit)?;
    Ok(analyte_mapping::vidas::map(settings, &message, input)?)
}
