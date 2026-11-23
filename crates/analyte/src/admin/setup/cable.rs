//! The serial cable settings Setup offers (ADR 0031): the standard lists, their plain words, the draft's choice and what differs from the machine definition's suggestion.
//!
//! The definition's `[transport.serial]` holds the manual's usual values as a
//! suggestion; the lab chooses the values its analyzer's own LIS (host)
//! settings screen shows, and Save writes all five into the site file's
//! `[serial]`. The config loader checks them, as it checks every layer.
use super::SetupError;
use super::wizard::Draft;
use analyte_config::{
    FlowControl, Parity, RecordTransport, Serial, SerialSetting, SerialValues, Stack, StreamKind,
};

/// The serial settings a validated stack opens its port with; none for a link
/// that is not a serial cable.
pub(in crate::admin) fn of(stack: &Stack) -> Option<&Serial> {
    let transport = match stack {
        Stack::Astm(RecordTransport::Stream(stack)) => stack.settings().transport(),
        Stack::Vidas(stack) => stack.settings().transport(),
        Stack::Autolyser(stack) => stack.settings().transport(),
        Stack::Record(stack) => stack.transport(),
        Stack::Astm(RecordTransport::Folder(_))
        | Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_) => return None,
    };
    match transport.kind() {
        StreamKind::Serial(serial) => Some(serial),
        StreamKind::Tcp(_) => None,
    }
}

const PARITIES: [(Parity, &str, &str, &str); 3] = [
    (Parity::None, "none", "None", "no parity"),
    (Parity::Odd, "odd", "Odd", "odd parity"),
    (Parity::Even, "even", "Even", "even parity"),
];

const FLOWS: [(FlowControl, &str, &str, &str); 3] = [
    (FlowControl::None, "none", "None", "no flow control"),
    (
        FlowControl::Hardware,
        "hardware",
        "Hardware (RTS/CTS)",
        "hardware flow control (RTS/CTS)",
    ),
    (
        FlowControl::Software,
        "software",
        "Software (XON/XOFF)",
        "software flow control (XON/XOFF)",
    ),
];

const DATA_BITS: [u8; 4] = [5, 6, 7, 8];
const STOP_BITS: [u8; 2] = [1, 2];

/// The setting's name for people.
pub(in crate::admin) const fn label(setting: SerialSetting) -> &'static str {
    match setting {
        SerialSetting::Baud => "Baud rate",
        SerialSetting::DataBits => "Data bits",
        SerialSetting::Parity => "Parity",
        SerialSetting::StopBits => "Stop bits",
        SerialSetting::FlowControl => "Flow control",
    }
}

/// Every value Setup offers for `setting`, as the form value and its label.
pub(in crate::admin::setup) fn options(setting: SerialSetting) -> Vec<(String, String)> {
    match setting {
        SerialSetting::Baud => analyte_transport::serial::BAUD_RATES
            .iter()
            .map(|baud| (baud.to_string(), baud.to_string()))
            .collect(),
        SerialSetting::DataBits => DATA_BITS
            .iter()
            .map(|bits| (bits.to_string(), bits.to_string()))
            .collect(),
        SerialSetting::Parity => PARITIES
            .iter()
            .map(|(_, value, shown, _)| ((*value).to_owned(), (*shown).to_owned()))
            .collect(),
        SerialSetting::StopBits => STOP_BITS
            .iter()
            .map(|bits| (bits.to_string(), bits.to_string()))
            .collect(),
        SerialSetting::FlowControl => FLOWS
            .iter()
            .map(|(_, value, shown, _)| ((*value).to_owned(), (*shown).to_owned()))
            .collect(),
    }
}

/// `values`' value of `setting` as the form and the command line write it.
pub(in crate::admin::setup) fn value(
    setting: SerialSetting,
    values: &SerialValues,
) -> Option<String> {
    match setting {
        SerialSetting::Baud => values.baud.map(|baud| baud.to_string()),
        SerialSetting::DataBits => values.data_bits.map(|bits| bits.to_string()),
        SerialSetting::Parity => values.parity.and_then(|parity| {
            PARITIES
                .iter()
                .find(|(known, ..)| *known == parity)
                .map(|(_, value, ..)| (*value).to_owned())
        }),
        SerialSetting::StopBits => values.stop_bits.map(|bits| bits.to_string()),
        SerialSetting::FlowControl => values.flow_control.and_then(|flow| {
            FLOWS
                .iter()
                .find(|(known, ..)| *known == flow)
                .map(|(_, value, ..)| (*value).to_owned())
        }),
    }
}

/// `serial`'s value of `setting` in words, such as "9600 baud" or "no parity".
fn word(setting: SerialSetting, serial: &Serial) -> String {
    match setting {
        SerialSetting::Baud => format!("{} baud", serial.baud),
        SerialSetting::DataBits => format!("{} data bits", serial.data_bits),
        SerialSetting::Parity => PARITIES
            .iter()
            .find(|(known, ..)| *known == serial.parity)
            .map_or_else(String::new, |(.., words)| (*words).to_owned()),
        SerialSetting::StopBits if serial.stop_bits == 1 => "1 stop bit".to_owned(),
        SerialSetting::StopBits => format!("{} stop bits", serial.stop_bits),
        SerialSetting::FlowControl => FLOWS
            .iter()
            .find(|(known, ..)| *known == serial.flow_control)
            .map_or_else(String::new, |(.., words)| (*words).to_owned()),
    }
}

/// The settings in plain words: "9600 baud, 8 data bits, no parity, 1 stop
/// bit, no flow control".
pub(in crate::admin::setup) fn words(serial: &Serial) -> String {
    SerialSetting::ALL
        .into_iter()
        .map(|setting| word(setting, serial))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `serial` as the values a layer writes, every one present.
pub(in crate::admin::setup) fn values(serial: &Serial) -> SerialValues {
    SerialValues {
        baud: Some(serial.baud),
        data_bits: Some(serial.data_bits),
        parity: Some(serial.parity),
        stop_bits: Some(serial.stop_bits),
        flow_control: Some(serial.flow_control),
    }
}

/// Each setting that differs from the definition's suggestion, in words:
/// "baud rate 19200 (the manual suggests 9600)".
fn differences(serial: &Serial, suggestion: Option<&SerialValues>) -> Vec<String> {
    let chosen = values(serial);
    serial
        .differs_from(suggestion)
        .into_iter()
        .map(|setting| {
            let now = value(setting, &chosen).unwrap_or_default();
            match suggestion.and_then(|suggested| value(setting, suggested)) {
                Some(suggested) => format!(
                    "{} {now} (the manual suggests {suggested})",
                    label(setting).to_lowercase()
                ),
                None => format!(
                    "{} {now} (the machine definition suggests none)",
                    label(setting).to_lowercase()
                ),
            }
        })
        .collect()
}

/// The settings in words and, when some differ, which: one line for the
/// command line, `doctor` and the machine's page.
pub(in crate::admin) fn summary(serial: &Serial, suggestion: Option<&SerialValues>) -> String {
    let differs = differences(serial, suggestion);
    if differs.is_empty() {
        format!("{}; as the manual suggests", words(serial))
    } else {
        format!(
            "{}; differs from the manual's suggestion: {}",
            words(serial),
            differs.join(", ")
        )
    }
}

/// The draft's typed value of `setting`, trimmed; none when empty.
pub(in crate::admin::setup) fn typed(draft: &Draft, setting: SerialSetting) -> Option<&str> {
    let text = match setting {
        SerialSetting::Baud => draft.baud.as_deref(),
        SerialSetting::DataBits => draft.data_bits.as_deref(),
        SerialSetting::Parity => draft.parity.as_deref(),
        SerialSetting::StopBits => draft.stop_bits.as_deref(),
        SerialSetting::FlowControl => draft.flow_control.as_deref(),
    };
    text.map(str::trim).filter(|text| !text.is_empty())
}

/// Sets the draft's value of `setting`.
pub(in crate::admin::setup) fn set(
    draft: &mut Draft,
    setting: SerialSetting,
    text: Option<String>,
) {
    let slot = match setting {
        SerialSetting::Baud => &mut draft.baud,
        SerialSetting::DataBits => &mut draft.data_bits,
        SerialSetting::Parity => &mut draft.parity,
        SerialSetting::StopBits => &mut draft.stop_bits,
        SerialSetting::FlowControl => &mut draft.flow_control,
    };
    *slot = text;
}

/// Whether the draft gives any cable setting.
pub(in crate::admin::setup) fn given(draft: &Draft) -> bool {
    SerialSetting::ALL
        .into_iter()
        .any(|setting| typed(draft, setting).is_some())
}

/// The draft's cable choice, read from the step 2 form or the command line:
/// each typed value, else the definition's suggestion. A value not on the
/// list is refused here or, for a number, by the config loader's own check.
pub(in crate::admin::setup) fn chosen(
    draft: &Draft,
    suggestion: Option<&SerialValues>,
) -> Result<SerialValues, SetupError> {
    let number = |setting, refusal| {
        typed(draft, setting)
            .map(|text| text.parse().map_err(|_| SetupError::Connection(refusal)))
            .transpose()
    };
    let typed_values = SerialValues {
        baud: typed(draft, SerialSetting::Baud)
            .map(|text| {
                text.parse()
                    .map_err(|_| SetupError::Connection("choose the baud rate from the list"))
            })
            .transpose()?,
        data_bits: number(SerialSetting::DataBits, "choose 5, 6, 7 or 8 data bits")?,
        parity: typed(draft, SerialSetting::Parity)
            .map(|text| {
                PARITIES
                    .iter()
                    .find(|(_, value, ..)| *value == text)
                    .map(|(parity, ..)| *parity)
                    .ok_or(SetupError::Connection(
                        "choose the parity: none, odd or even",
                    ))
            })
            .transpose()?,
        stop_bits: number(SerialSetting::StopBits, "choose 1 or 2 stop bits")?,
        flow_control: typed(draft, SerialSetting::FlowControl)
            .map(|text| {
                FLOWS
                    .iter()
                    .find(|(_, value, ..)| *value == text)
                    .map(|(flow, ..)| *flow)
                    .ok_or(SetupError::Connection(
                        "choose the flow control: none, hardware or software",
                    ))
            })
            .transpose()?,
    };
    Ok(typed_values.over(suggestion))
}

/// The settings the chosen values still lack: neither typed nor suggested.
pub(in crate::admin::setup) fn missing(
    draft: &Draft,
    suggestion: Option<&SerialValues>,
) -> Vec<SerialSetting> {
    let known = suggestion.cloned().unwrap_or_default();
    SerialSetting::ALL
        .into_iter()
        .filter(|setting| typed(draft, *setting).is_none() && value(*setting, &known).is_none())
        .collect()
}

/// The serial values a site file chooses, as written; none when it writes
/// none or they cannot be read (the loader then says why).
pub(in crate::admin::setup) fn saved(site: &toml::Table) -> Option<SerialValues> {
    site.get("serial")?.clone().try_into().ok()
}

/// Fills each empty cable setting of `draft` from the saved site file, so the
/// saved choice is kept unless changed, as every other saved choice is.
pub(in crate::admin::setup) fn keep_saved(draft: &mut Draft, site: &toml::Table) {
    let Some(saved) = saved(site) else {
        return;
    };
    for setting in SerialSetting::ALL {
        if typed(draft, setting).is_none() {
            set(draft, setting, value(setting, &saved));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0031: every offered value reads back as itself, so a value shown on
    // step 2 is the value saved.
    #[test]
    fn every_offered_value_reads_back() -> Result<(), SetupError> {
        for setting in SerialSetting::ALL {
            for (offered, _) in options(setting) {
                let mut draft = Draft::default();
                set(&mut draft, setting, Some(offered.clone()));
                let picked = chosen(&draft, None)?;
                assert_eq!(value(setting, &picked), Some(offered), "{setting}");
            }
        }
        Ok(())
    }

    // ADR 0031 decision 2: a typed value wins, the suggestion fills the rest.
    #[test]
    fn typed_values_win_over_the_suggestion() -> Result<(), SetupError> {
        let suggestion = SerialValues {
            baud: Some(9600),
            data_bits: Some(8),
            parity: Some(Parity::None),
            stop_bits: Some(1),
            flow_control: None,
        };
        let draft = Draft {
            baud: Some(" 19200 ".into()),
            ..Draft::default()
        };
        let picked = chosen(&draft, Some(&suggestion))?;
        assert_eq!(picked.baud, Some(19200));
        assert_eq!(picked.parity, Some(Parity::None));
        assert_eq!(
            missing(&draft, Some(&suggestion)),
            [SerialSetting::FlowControl]
        );
        for (setting, text) in [
            (SerialSetting::Baud, "fast"),
            (SerialSetting::Parity, "mark"),
            (SerialSetting::FlowControl, "dtr"),
            (SerialSetting::StopBits, "1.5"),
        ] {
            let mut draft = Draft::default();
            set(&mut draft, setting, Some(text.into()));
            assert!(
                matches!(chosen(&draft, None), Err(SetupError::Connection(_))),
                "{setting} {text}"
            );
        }
        Ok(())
    }

    // ADR 0031: the words name each setting the way the analyzer screens do.
    #[test]
    fn words_and_differences() {
        let serial = Serial {
            baud: 19200,
            data_bits: 7,
            parity: Parity::Even,
            stop_bits: 2,
            flow_control: FlowControl::Hardware,
        };
        assert_eq!(
            words(&serial),
            "19200 baud, 7 data bits, even parity, 2 stop bits, hardware flow control (RTS/CTS)"
        );
        let suggestion = SerialValues {
            baud: Some(9600),
            data_bits: Some(7),
            parity: Some(Parity::Even),
            stop_bits: Some(2),
            flow_control: None,
        };
        assert_eq!(
            differences(&serial, Some(&suggestion)),
            [
                "baud rate 19200 (the manual suggests 9600)",
                "flow control hardware (the machine definition suggests none)"
            ]
        );
    }
}
