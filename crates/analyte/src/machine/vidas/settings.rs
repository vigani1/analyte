//! Translates installed VIDAS framing, sample identity and dialog settings.
use crate::machine::receive::custody::ms;
use crate::reading::Error;
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

pub(crate) fn sample_limit(
    settings: &analyte_config::VidasStack,
) -> analyte_vidas::syntax::SampleLimit {
    match settings.policy().sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    }
}

pub(crate) fn dialog(settings: &analyte_config::VidasStack) -> analyte_dialog::vidas::Config {
    let installed = settings.policy();
    analyte_dialog::vidas::Config {
        ack: ms(installed.acknowledgment_ms),
        inter_byte: ms(installed.interbyte_ms),
        session: ms(installed.session_ms),
        commit: ms(installed.commit_ms),
        retries: installed.retries,
        backoff: ms(installed.backoff_ms),
    }
}
