//! Asks for missing machine setup choices at the terminal.
use super::super::{
    Failure,
    ask::{self, Ask},
    problem, say,
};
use crate::admin::setup::{
    Definition, Setup, SetupError, Shipped, cable, choices, definitions, lab_apps, level_and_state,
    plan, site,
    wizard::{Draft, host_choices},
};
use crate::admin::view::format;
use crate::machine::link::status::Link;
use analyte_config::{DefinitionDetails, SerialSetting, TransportKind};
use analyte_model::Identifier;
use analyte_transport::serial;

/// For a machine that connects to Analyte whose devices are not chosen yet,
/// asks at a terminal which devices it accepts, as step 2's field does (ADR
/// 0026). Returns whether it asked.
pub(super) fn ask_peers(
    setup: &Setup,
    id: &str,
    draft: &mut Draft,
    ask: Ask,
) -> Result<bool, Failure> {
    if ask == Ask::Never || draft.allowed_peers.is_some() {
        return Ok(false);
    }
    let plan = plan::plan(setup, id, draft)?;
    if Link::of(plan.validated.effective().stack()) != Link::TcpServer {
        return Ok(false);
    }
    say(concat!(
        "Any device that reaches this port could send results as this machine. ",
        "Run analyte machines test to see the analyzer's address."
    ))?;
    let typed = ask.line(
        "Only accept these devices: the analyzer's IP address (several with commas; empty accepts any device)",
    )?;
    draft.allowed_peers = Some(typed.unwrap_or_default());
    Ok(true)
}

/// Asks at a terminal for each choice the draft still lacks, in step order:
/// the model, the connection, the lab app. Returns whether anything was asked.
pub(super) fn fill(setup: &Setup, id: &str, draft: &mut Draft, ask: Ask) -> Result<bool, Failure> {
    if ask == Ask::Never {
        return Ok(false);
    }
    let mut asked = false;
    let (_, installed) = site::installed(setup.config(), id)?;
    let list = definitions(&setup.seams)?;
    let records_only = plan::recording::chosen(draft);
    let model = match plan::model_of(&list, draft, &installed) {
        Ok(model) => model,
        Err(SetupError::DefinitionFile(error)) => {
            return Err(SetupError::DefinitionFile(error).into());
        }
        Err(_) => {
            asked = true;
            let choices: Vec<ask::Choice> = list
                .iter()
                .filter(|definition| {
                    matches!(definition.state, Shipped::Ready { .. })
                        || (records_only
                            && matches!(definition.state, Shipped::Blocked { record: Some(_) }))
                })
                .map(|definition| ask::Choice {
                    value: definition.key.clone(),
                    label: model_label(definition),
                })
                .collect();
            draft.definition = ask.choose("Step 1, Machine: which model is it?", &choices)?;
            plan::model_of(&list, draft, &installed)?
        }
    };
    let kind = model.details.kind;
    if plan::connection_of(kind, draft).is_err() {
        asked = true;
        ask_connection(setup, kind, draft, ask)?;
    }
    asked |= if records_only {
        ask_recording(&model.details, draft, ask)?
    } else {
        ask_model_choices(&model.details, draft, ask)?
    };
    if kind == TransportKind::Serial {
        for setting in cable::missing(draft, model.details.serial.as_ref()) {
            asked = true;
            let choices: Vec<ask::Choice> = cable::options(setting)
                .into_iter()
                .map(|(value, label)| ask::Choice { value, label })
                .collect();
            let question = format!(
                "Step 2, Cable: {}? The model suggests none; match the analyzer's LIS/host settings screen",
                cable::label(setting).to_lowercase()
            );
            let chosen = ask.choose(&question, &choices)?;
            cable::set(draft, setting, chosen);
        }
    }
    let scan = lab_apps::scan(setup.config())?;
    let known = draft
        .lab_app
        .as_deref()
        .is_some_and(|name| scan.get(name.trim()).is_some());
    // A record-only machine sends to no lab app (ADR 0034).
    if !known && !records_only {
        asked = true;
        let choices: Vec<ask::Choice> = scan
            .known
            .iter()
            .map(|known| ask::Choice {
                value: known.settings.name.as_str().to_owned(),
                label: format!(
                    "{} - {}",
                    known.settings.name.as_str(),
                    known.settings.base_url.as_str()
                ),
            })
            .collect();
        if choices.is_empty() {
            say(
                "There is no lab app yet; add one first with: analyte lab-apps add <name> --address <https://...>",
            )?;
        }
        draft.lab_app = ask.choose("Which lab app does it send results to?", &choices)?;
    }
    Ok(asked)
}

/// Asks the settings a normal model declares when no saved answer exists.
fn ask_model_choices(
    definition: &DefinitionDetails,
    draft: &mut Draft,
    ask: Ask,
) -> Result<bool, Failure> {
    let mut asked = false;
    if definition.asks_autolyser_poll && draft.poll_seconds.is_none() {
        asked = true;
        let range = analyte_config::AUTOLYSER_POLL_SECONDS;
        draft.poll_seconds = ask.line(&format!(
            "Step 2, Analyzer settings: seconds between report polls ({} to {})",
            range.start(),
            range.end(),
        ))?;
    }
    for choice in choices::missing(draft, &definition.choices) {
        asked = true;
        let options: Vec<ask::Choice> = choice
            .values
            .iter()
            .map(|value| ask::Choice {
                value: value.value.as_str().to_owned(),
                label: choices::value_words(&value.label, value.version_evidence),
            })
            .collect();
        let question = format!(
            "Step 2, Analyzer settings: {}? The model suggests none; {}",
            choice.label.as_str(),
            choices::hint(choice.kind).to_lowercase()
        );
        if let Some(value) = ask.choose(&question, &options)? {
            draft.choices.insert(choice.name.as_str().to_owned(), value);
        }
    }
    Ok(asked)
}

/// Record only at the terminal: the capture profile's questions the draft
/// does not answer yet (ADR 0034). Returns whether anything was asked.
fn ask_recording(
    definition: &DefinitionDetails,
    draft: &mut Draft,
    ask: Ask,
) -> Result<bool, Failure> {
    let empty = |value: &Option<String>| value.as_deref().is_none_or(|v| v.trim().is_empty());
    let mut asked = false;
    match definition.protocol.as_str() {
        "autolyser" => {
            if empty(&draft.poll_seconds) {
                asked = true;
                let range = analyte_config::recording::POLL_SECONDS;
                draft.poll_seconds = ask.line(&format!(
                    "Step 2, Recording: seconds between asks for the next report ({} to {})",
                    range.start(),
                    range.end(),
                ))?;
            }
            if empty(&draft.controls) {
                asked = true;
                let options = [("yes", "Yes"), ("no", "No")].map(|(value, label)| ask::Choice {
                    value: value.into(),
                    label: label.into(),
                });
                draft.controls = ask.choose(
                    "Step 2, Recording: do the analyzer's reports carry its controls?",
                    &options,
                )?;
            }
        }
        "vidas" if empty(&draft.acknowledgment) => {
            asked = true;
            let options = [
                ("literal", "Literal (after the checksum)"),
                ("alternate", "Alternate (after ETX)"),
            ]
            .map(|(value, label)| ask::Choice {
                value: value.into(),
                label: label.into(),
            });
            draft.acknowledgment = ask.choose(
                "Step 2, Recording: which acknowledgment does the VIDAS PC use?",
                &options,
            )?;
        }
        _ => {}
    }
    if let Some(mode) = &definition.analyzer_mode
        && empty(&draft.analyzer_mode)
    {
        asked = true;
        let options: Vec<ask::Choice> = mode
            .values
            .iter()
            .map(|value| ask::Choice {
                value: value.value.as_str().into(),
                label: value.label.as_str().into(),
            })
            .collect();
        draft.analyzer_mode = ask.choose(
            &format!("Step 2, Recording: {}?", mode.label.as_str()),
            &options,
        )?;
    }
    Ok(asked)
}

/// A shipped model as a numbered choice shows it.
fn model_label(definition: &Definition) -> String {
    let (checked, _, _) = level_and_state(definition);
    format!(
        "{} ({}) - {}",
        definition.display, definition.key, checked.shown
    )
}

/// Step 2 at the terminal: a detected port or a typed name, an address and a
/// port number, or a folder, by the model's kind of link.
fn ask_connection(
    setup: &Setup,
    kind: TransportKind,
    draft: &mut Draft,
    ask: Ask,
) -> Result<(), Failure> {
    *draft = Draft {
        definition: draft.definition.take(),
        lab_app: draft.lab_app.take(),
        mode: draft.mode.take(),
        poll_seconds: draft.poll_seconds.take(),
        controls: draft.controls.take(),
        acknowledgment: draft.acknowledgment.take(),
        analyzer_mode: draft.analyzer_mode.take(),
        repeated_results: draft.repeated_results.take(),
        baud: draft.baud.take(),
        data_bits: draft.data_bits.take(),
        parity: draft.parity.take(),
        stop_bits: draft.stop_bits.take(),
        flow_control: draft.flow_control.take(),
        ..Draft::default()
    };
    if kind != TransportKind::Serial {
        for setting in SerialSetting::ALL {
            cable::set(draft, setting, None);
        }
    }
    match kind {
        TransportKind::Serial => {
            let ports = match (setup.seams.ports)() {
                Ok(ports) => ports,
                Err(error) => {
                    problem(&format!(
                        "Could not list serial ports: {error}. You can enter a port name."
                    ))?;
                    Vec::new()
                }
            };
            let mut choices: Vec<ask::Choice> = ports
                .iter()
                .take(serial::LIST_LIMIT)
                .map(|port| ask::Choice {
                    value: port.factory_serial().map_or_else(
                        || format!("name:{}", port.name),
                        |number| format!("usb:{number}"),
                    ),
                    label: port_line(port, None),
                })
                .collect();
            choices.push(ask::Choice {
                value: String::new(),
                label: "Type a port name".into(),
            });
            match ask
                .choose("Step 2, Connection: which serial port?", &choices)?
                .as_deref()
            {
                Some("") => draft.port_name = ask.line("Port name, such as COM3")?,
                Some(port) => draft.port = Some(port.to_owned()),
                None => {}
            }
        }
        TransportKind::TcpServer => {
            draft.host = ask_listen_address(setup, ask)?;
            draft.tcp_port = ask.line("Port number, 1 to 65535")?;
        }
        TransportKind::TcpClient => {
            draft.host =
                ask.line("Step 2, Connection: the machine's IP address, such as 192.168.1.20")?;
            draft.tcp_port = ask.line("Port number, 1 to 65535")?;
        }
        TransportKind::WatchedFolder => {
            draft.folder = ask.line("Step 2, Connection: the folder Analyte reads results from")?;
        }
    }
    Ok(())
}

/// Step 2 for a machine that connects to Analyte: this PC's addresses and
/// every network card (0.0.0.0), as step 2 lists them, or a typed address.
fn ask_listen_address(setup: &Setup, ask: Ask) -> Result<Option<String>, Failure> {
    let found = match (setup.seams.addresses)() {
        Ok(found) => found,
        Err(error) => {
            say(&format::sentence(&error.to_string()))?;
            Vec::new()
        }
    };
    let mut choices: Vec<ask::Choice> = host_choices(&found, "")
        .into_iter()
        .map(|host| ask::Choice {
            value: host.value,
            label: format!("{} - {}", host.address, host.meta),
        })
        .collect();
    choices.push(ask::Choice {
        value: String::new(),
        label: "Type an address".into(),
    });
    let question = "Step 2, Connection: the machine connects to this PC. Which address does Analyte listen on?";
    Ok(match ask.choose(question, &choices)?.as_deref() {
        Some("") => ask.line("IP address, such as 192.168.1.20")?,
        chosen => chosen.map(str::to_owned),
    })
}

/// One detected serial port in step 2's words: its adapter, how it is
/// remembered, and the running machine that uses it, if any.
pub(super) fn port_line(port: &serial::Listed, used_by: Option<&Identifier>) -> String {
    let remembered = match port.factory_serial() {
        Some(number) => format!("remembered by its serial number {number} (--usb-serial {number})"),
        None => format!(
            "no serial number; remembered by its port name (--serial-port {})",
            port.name
        ),
    };
    format!(
        "{} - {} - {}",
        port.name,
        port.description.as_deref().unwrap_or("unknown adapter"),
        used_by.map_or(remembered, |holder| format!("used by {}", holder.as_str()))
    )
}
