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
use analyte_i18n::{message as tr_args, text as tr};
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
    say(tr(
        "setup-any-device-that-reaches-this-port-could-send-results",
    ))?;
    let typed = ask.line(tr(
        "setup-only-accept-these-devices-the-analyzer-s-ip-address",
    ))?;
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
            draft.definition =
                ask.choose(tr("setup-step-1-machine-which-model-is-it"), &choices)?;
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
            let question = tr_args(
                "setup-step-2-cable-the-model-suggests-none-match-the",
                [(
                    "to-lowercase",
                    (cable::label(setting).to_lowercase()).to_string().into(),
                )],
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
        draft.lab_app = ask.choose(tr("setup-which-lab-app-does-it-send-results-to"), &choices)?;
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
        draft.poll_seconds = ask.line(&tr_args(
            "setup-step-2-analyzer-settings-seconds-between-report-polls-to",
            [
                ("start", (range.start()).to_string().into()),
                ("end", (range.end()).to_string().into()),
            ],
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
        let question = tr_args(
            "setup-step-2-analyzer-settings-the-model-suggests-none",
            [
                ("label", (choice.label.as_str()).to_string().into()),
                (
                    "to-lowercase",
                    (choices::hint(choice.kind).to_lowercase())
                        .to_string()
                        .into(),
                ),
            ],
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
                draft.poll_seconds = ask.line(&tr_args(
                    "setup-step-2-recording-seconds-between-asks-for-the-next",
                    [
                        ("start", (range.start()).to_string().into()),
                        ("end", (range.end()).to_string().into()),
                    ],
                ))?;
            }
            if empty(&draft.controls) {
                asked = true;
                let options =
                    [("yes", tr("setup-yes")), ("no", tr("setup-no"))].map(|(value, label)| {
                        ask::Choice {
                            value: value.into(),
                            label: label.into(),
                        }
                    });
                draft.controls = ask.choose(
                    tr("setup-step-2-recording-do-the-analyzer-s-reports-carry"),
                    &options,
                )?;
            }
        }
        "vidas" if empty(&draft.acknowledgment) => {
            asked = true;
            let options = [
                ("literal", tr("setup-literal-after-the-checksum")),
                ("alternate", tr("setup-alternate-after-etx")),
            ]
            .map(|(value, label)| ask::Choice {
                value: value.into(),
                label: label.into(),
            });
            draft.acknowledgment = ask.choose(
                tr("setup-step-2-recording-which-acknowledgment-does-the-vidas-pc"),
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
                    problem(&tr_args(
                        "setup-could-not-list-serial-ports-you-can-enter-a",
                        [("error", (error).to_string().into())],
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
                        |number| tr_args("setup-usb", [("number", (number).to_string().into())]),
                    ),
                    label: port_line(port, None),
                })
                .collect();
            choices.push(ask::Choice {
                value: String::new(),
                label: tr("setup-type-a-port-name").into(),
            });
            match ask
                .choose(tr("setup-step-2-connection-which-serial-port"), &choices)?
                .as_deref()
            {
                Some("") => {
                    draft.port_name = ask.line(tr("setup-port-name-such-as-com3"))?;
                }
                Some(port) => draft.port = Some(port.to_owned()),
                None => {}
            }
        }
        TransportKind::TcpServer => {
            draft.host = ask_listen_address(setup, ask)?;
            draft.tcp_port = ask.line(tr("setup-port-number-1-to-65535"))?;
        }
        TransportKind::TcpClient => {
            draft.host = ask.line(tr("setup-step-2-connection-the-machine-s-ip-address-such"))?;
            draft.tcp_port = ask.line(tr("setup-port-number-1-to-65535"))?;
        }
        TransportKind::WatchedFolder => {
            draft.folder = ask.line(tr(
                "setup-step-2-connection-the-folder-analyte-reads-results-from",
            ))?;
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
        label: tr("setup-type-an-address").into(),
    });
    let question = tr("setup-step-2-connection-the-machine-connects-to-this-pc");
    Ok(match ask.choose(question, &choices)?.as_deref() {
        Some("") => ask.line(tr("setup-ip-address-such-as-192-168-1-20"))?,
        chosen => chosen.map(str::to_owned),
    })
}

/// One detected serial port in step 2's words: its adapter, how it is
/// remembered, and the running machine that uses it, if any.
pub(super) fn port_line(port: &serial::Listed, used_by: Option<&Identifier>) -> String {
    let remembered = match port.factory_serial() {
        Some(number) => tr_args(
            "setup-remembered-by-its-serial-number-usb-serial",
            [("number", (number).to_string().into())],
        ),
        None => tr_args(
            "setup-no-serial-number-remembered-by-its-port-name-serial",
            [("name", (port.name).to_string().into())],
        ),
    };
    format!(
        "{} - {} - {}",
        port.name,
        port.description.as_deref().unwrap_or("unknown adapter"),
        used_by.map_or(remembered, |holder| tr_args(
            "setup-used-by-4",
            [("holder", (holder.as_str()).to_string().into())]
        ))
    )
}
