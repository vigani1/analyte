//! Translates installed HL7 syntax settings into parser options.
use analyte_config::{Encoding, Escapes, Hl7OptionsSource, SyntaxLimits, Terminator};
use analyte_dialog::hl7::Config;
use std::time::Duration;
pub(crate) fn options_from_values(
    encoding: Encoding,
    terminator: Terminator,
    escapes: Escapes,
    limits: &SyntaxLimits,
) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: false,
        charset: match encoding {
            Encoding::Ascii => analyte_hl7::Charset::Ascii,
            Encoding::Latin1 => analyte_hl7::Charset::Latin1,
            Encoding::Utf8 => analyte_hl7::Charset::Utf8,
        },
        terminator: match terminator {
            Terminator::Cr => analyte_hl7::Terminator::Cr,
            Terminator::CrLf => analyte_hl7::Terminator::CrLf,
            Terminator::Lf => analyte_hl7::Terminator::Lf,
        },
        escapes: match escapes {
            Escapes::Reject => analyte_hl7::EscapePolicy::Reject,
            Escapes::Preserve => analyte_hl7::EscapePolicy::Preserve,
        },
        limits: analyte_hl7::Limits {
            message_bytes: limits.bytes(),
            segments: limits.records(),
            fields: limits.fields(),
            repetitions: limits.repetitions(),
        },
    }
}

pub(crate) fn options(syntax: &analyte_config::Hl7Syntax) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: syntax.allow_utf8_in_v231(),
        ..options_from_values(
            syntax.encoding(),
            syntax.terminator(),
            syntax.escapes(),
            syntax.limits(),
        )
    }
}
pub(crate) fn effective_options(
    effective: &analyte_config::Effective,
) -> Option<analyte_hl7::Options> {
    effective
        .stack()
        .hl7_options_source()
        .map(|source| match source {
            Hl7OptionsSource::Syntax(syntax) => options(syntax),
            Hl7OptionsSource::Law(settings) => crate::machine::law::settings::options(settings),
        })
}

pub(crate) fn dialog(
    settings: &analyte_config::Hl7Stack,
) -> Result<(analyte_hl7::Options, Config), super::Error> {
    let mut options = options(settings.settings().syntax());
    options.limits.message_bytes = options
        .limits
        .message_bytes
        .min(settings.settings().frame_limit());
    let config = Config::new(
        options,
        Duration::from_millis(u64::from(
            settings.settings().timeouts().receive_timeout_ms(),
        )),
        1,
    )?;
    Ok((options, config))
}

pub(crate) fn frame_limit(
    settings: &analyte_config::Hl7Stack,
) -> Result<analyte_framing::hl7::Limit, analyte_framing::hl7::Error> {
    analyte_framing::hl7::Limit::new(settings.settings().frame_limit())
}
pub(crate) fn mapping<'a>(
    syntax: &'a analyte_config::Hl7Syntax,
    mappings: &'a analyte_config::Mappings,
    declarations: &'a [analyte_config::MachineDeparture],
) -> analyte_mapping::hl7::Settings<'a> {
    analyte_mapping::hl7::Settings {
        syntax,
        mappings,
        declarations,
    }
}
