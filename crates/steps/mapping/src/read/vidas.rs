//! Reads VIDAS packets using the installed framing and sample identity policy.
use super::Error;
use analyte_config::Stack;
pub(super) fn read(
    config: &analyte_config::Validated,
    input: crate::Input,
    raw: &[u8],
    settings: &analyte_config::VidasStack,
) -> Result<crate::Output, Error> {
    let packet = analyte_framing::vidas::decode(raw, framing(config)?)?;
    let limit = match settings.policy().sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    };
    let message = analyte_vidas::syntax::Message::parse(std::str::from_utf8(&packet.body)?, limit)?;
    Ok(crate::map_vidas(
        config,
        input.receipt,
        &message,
        input.received_at,
    )?)
}
/// Translates installed VIDAS framing choices once for live and retained input.
/// # Errors
/// Rejects another stack or invalid framing settings.
pub fn framing(
    config: &analyte_config::Validated,
) -> Result<analyte_framing::vidas::Config, Error> {
    use analyte_framing::vidas::{Checksum, Config, Mode};
    let settings = match config.effective().stack() {
        Stack::Vidas(settings) => settings,
        Stack::Astm(analyte_config::RecordTransport::Stream(_))
        | Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Autolyser(_)
        | Stack::Astm(analyte_config::RecordTransport::Folder(_))
        | Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => {
            return Err(Error::Context(super::Context::VidasSettingsMissing));
        }
    };
    let installed = &settings.policy();
    Ok(Config::new(
        if installed.ack_point == analyte_config::AckPoint::Etx {
            Mode::Alternate
        } else {
            Mode::Literal
        },
        installed.chunk,
        settings.frame_limit(),
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
