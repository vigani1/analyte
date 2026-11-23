//! Reads analyzer software metadata through the installed framing and syntax.
use analyte_config::Validated;
use analyte_config::software_version::{Selector, reported};
use analyte_model::{Digest, Identifier};

/// Returns an unexpected version, with absent metadata preserved as unknown.
pub(crate) fn observe(config: &Validated, raw: &[u8]) -> Option<(Digest, Option<Identifier>)> {
    let check = config.effective().machine().software_version()?;
    let version = read(config, raw, check.selector);
    if version
        .as_ref()
        .is_some_and(|version| check.checked.contains(version))
    {
        return None;
    }
    Some((check.revision, version))
}

fn read(config: &Validated, raw: &[u8], selector: Selector) -> Option<Identifier> {
    let payload = super::protocols::payload(config, raw).ok()?;
    match selector {
        Selector::AstmSenderThirdComponent => {
            let syntax = config.effective().stack().astm_syntax()?;
            let messages = crate::reading::astm_messages(&payload, syntax).ok()?;
            let [message] = messages.as_slice() else {
                return None;
            };
            let field = message.records().first()?.field(5)?;
            let [components] = field.repeats() else {
                return None;
            };
            let component = components.get(2)?;
            if component.has_control_character() || component.has_unsupported_escape() {
                return None;
            }
            reported(component.raw())
        }
        Selector::Hl7SendingApplication | Selector::Hl7SendingFacility => {
            let options = super::hl7::settings::effective_options(config.effective())?;
            let message = analyte_hl7::Message::parse(&payload, options).ok()?;
            hl7_version(
                &message,
                if matches!(selector, Selector::Hl7SendingApplication) {
                    3
                } else {
                    4
                },
            )
        }
    }
}

fn hl7_version(message: &analyte_hl7::Message, position: usize) -> Option<Identifier> {
    let field = message.segments().next()?.field(position)?;
    if field.is_null() || field.is_truncated() {
        return None;
    }
    let mut repetitions = field.repetitions();
    let version = repetitions.next()?.components().next()?;
    if repetitions.next().is_some() || version.is_truncated() {
        return None;
    }
    reported(version.raw())
}

/// Operator wording shared by Overview and the machine detail.
pub(crate) fn message(
    machine: &Identifier,
    check: &analyte_config::software_version::Check,
    warning: &analyte_store::SoftwareWarning,
) -> String {
    let checked = check
        .checked
        .iter()
        .map(Identifier::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let report = warning.reported.as_ref().map_or_else(
        || {
            format!(
                "The software version reported by {} is unknown",
                machine.as_str()
            )
        },
        |version| {
            format!(
                "The {} reports software {}",
                machine.as_str(),
                version.as_str()
            )
        },
    );
    format!(
        "{report}; its settings were checked with {checked}. Check its settings before relying on it."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn config(astm: bool, selector: Option<&str>) -> Result<Validated, analyte_config::Error> {
        let (definition, endpoint) = if astm {
            (
                include_str!("../../../core/config/examples/astm.toml"),
                "COM9",
            )
        } else {
            (
                include_str!("../../../core/config/examples/hl7.toml"),
                "127.0.0.1:5600",
            )
        };
        let mut definition = definition.replace("firmware = \"unknown\"", "firmware = \"1.0\"");
        if let Some(selector) = selector {
            definition.push_str(&format!(
                "\n[software_version]\nselector = '{selector}'\nsource = 'Synthetic selector fixture'\n",
            ));
        }
        analyte_config::Draft::load(
            ("machine.toml", &definition),
            (
                "site.toml",
                &include_str!("../../../core/config/examples/site.toml")
                    .replace("127.0.0.1:5600", endpoint),
            ),
            ("overrides.toml", "schema_version = 1"),
        )?
        .validate()
    }

    // BioCare Biolyte Service Manual 4.2A Appendix VI, header table (ASTM E1394-91 section 7.1.5).
    #[test]
    fn astm_reads_the_third_sender_component() -> TestResult {
        use analyte_framing::astm::{FrameNumber, Limit, encode};
        let config = config(true, Some("astm_sender_third_component"))?;
        let mut transfer: Vec<u8> = encode(
            b"H|\\^&|||Example^Analyzer^2.0^A0001\r",
            FrameNumber::FIRST,
            Limit::default(),
        )?
        .flatten()
        .collect();
        transfer.extend(encode(b"L|1|N\r", FrameNumber::FIRST.next(), Limit::default())?.flatten());
        let (_, version) = observe(&config, &transfer).ok_or("expected warning")?;
        assert_eq!(version.as_ref().map(Identifier::as_str), Some("2.0"));
        Ok(())
    }

    // HL7 2.5.1 section 2.15.9 (MSH-3 and MSH-4 wire positions), synthetic vendor placement.
    #[test]
    fn hl7_reads_only_the_selected_metadata_and_preserves_unknown() -> TestResult {
        let application = config(false, Some("hl7_sending_application"))?;
        let facility = config(false, Some("hl7_sending_facility"))?;
        let without = config(false, None)?;
        let message = b"\x0bMSH|^~\\&|1.0^ns|2.0|R|F|20261116||ORU^R01^ORU_R01|one|P|2.5.1\r\x1c\r";
        assert!(observe(&application, message).is_none());
        assert!(observe(&without, message).is_none());
        let (_, version) = observe(&facility, message).ok_or("expected warning")?;
        assert_eq!(version.as_ref().map(Identifier::as_str), Some("2.0"));
        let garbled = b"\x0bMSH|^~\\&|custom|malformed version|R|F|20261116||ORU^R01^ORU_R01|one|P|2.5.1\r\x1c\r";
        assert!(matches!(observe(&facility, garbled), Some((_, None))));
        assert!(config(true, Some("hl7_sending_facility")).is_err());
        Ok(())
    }
}
