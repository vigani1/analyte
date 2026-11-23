//! Translates installed ASTM syntax and dialog settings.
use analyte_dialog::astm::{Config, Role, Timers};
use std::time::Duration;
/// Supplies the LIS02 parser with the configured resource ceilings.
#[must_use]
pub(crate) fn parser_limits(syntax: &analyte_config::AstmSyntax) -> analyte_astm::Limits {
    analyte_astm::Limits {
        bytes: syntax.limits().bytes(),
        record: syntax.limits().record_bytes(),
        fields: syntax.limits().fields(),
        repeats: syntax.limits().repetitions(),
        components: syntax.limits().components(),
    }
}

pub(crate) fn dialog(
    settings: &analyte_config::AstmStack,
    commit: Duration,
) -> Result<Config, super::Error> {
    let timers = Timers {
        establish: Duration::from_millis(u64::from(
            settings.settings().timeouts().send_timeout_ms(),
        )),
        frame: Duration::from_millis(u64::from(settings.settings().timeouts().send_timeout_ms())),
        receive: Duration::from_millis(u64::from(
            settings.settings().timeouts().receive_timeout_ms(),
        )),
        commit,
        ..Timers::default()
    };
    let config = Config::new(
        settings.frame_limit(),
        timers,
        match settings.contention_priority() {
            analyte_config::Priority::Host => Role::Instrument,
            analyte_config::Priority::Instrument => Role::Computer,
        },
        settings.settings().syntax().limits().bytes(),
        settings.settings().syntax().limits().records(),
        analyte_dialog::astm::Interrupt::Honor,
    )?
    .with_repeat_enq(settings.repeat_enq())
    .with_record_boundaries(record_boundaries(settings))
    .with_checksum_hex(checksum_policy(settings));
    Ok(config)
}

pub(crate) fn record_boundaries(
    settings: &analyte_config::AstmStack,
) -> analyte_framing::astm::RecordBoundaries {
    if settings.packed_records() {
        analyte_framing::astm::RecordBoundaries::Packed
    } else {
        analyte_framing::astm::RecordBoundaries::Separate
    }
}
// ADR 0002: ChecksumHex owns stable serialized configuration spellings saved
// with receipts; HexPolicy owns framing behavior without a serde dependency.
// Translate once at host assembly, keeping configuration out of the framer.
pub(crate) fn checksum_policy(
    settings: &analyte_config::AstmStack,
) -> analyte_framing::astm::HexPolicy {
    match settings.checksum_hex() {
        analyte_config::ChecksumHex::Strict => analyte_framing::astm::HexPolicy::Strict,
        analyte_config::ChecksumHex::AcceptLowercase => {
            analyte_framing::astm::HexPolicy::AcceptLowercase
        }
    }
}
pub(crate) fn mapping<'a>(
    syntax: &'a analyte_config::AstmSyntax,
    mappings: &'a analyte_config::Mappings,
) -> analyte_mapping::astm::Settings<'a> {
    analyte_mapping::astm::Settings { syntax, mappings }
}
