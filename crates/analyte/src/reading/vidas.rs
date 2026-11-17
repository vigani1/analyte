//! Reads VIDAS packets using the installed framing and sample identity policy.
use super::Error;
pub(super) fn read(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::VidasStack,
) -> Result<analyte_mapping::Output, Error> {
    let packet = analyte_framing::vidas::decode(raw, framing(settings)?)?;
    let limit = match settings.policy().sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    };
    let message = analyte_vidas::syntax::Message::parse(std::str::from_utf8(&packet.body)?, limit)?;
    Ok(analyte_mapping::vidas::map(settings, &message, input)?)
}
/// Translates installed VIDAS framing choices once for live and retained input.
/// # Errors
/// Rejects invalid framing settings.
pub(crate) fn framing(
    settings: &analyte_config::VidasStack,
) -> Result<analyte_framing::vidas::Config, Error> {
    use analyte_framing::vidas::{Checksum, Config, Mode};
    let installed = &settings.policy();
    Ok(Config::new(
        if installed.ack_point == analyte_config::AckPoint::Etx {
            Mode::Alternate
        } else {
            Mode::Literal
        },
        installed.chunk,
        settings.settings().frame_limit(),
        &installed.suffix,
        Checksum {
            include_stx: installed.include_stx.into(),
            include_suffix: installed.include_suffix.into(),
            hex: analyte_framing::vidas::HexOptions {
                uppercase: installed.hex_case.into(),
                accept_either_case: installed.checksum_acceptance.into(),
            },
        },
    )?)
}
