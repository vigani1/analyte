//! Renders the connection settings and lab app choices in step 2.

use super::{
    super::{
        Address, SetupError, cable, choices, definitions, lab_apps,
        plan::{current_lab_app, holder, model_of},
        serial, site,
    },
    Draft, Rendered, Step, frame,
    page::{self, Cable, CableOption, ConnectionStep, LabApp, Picker, Port, Setting},
    wizard,
};
use crate::admin::{
    server::{Context, sessions::Session},
    view::{format, parts::Hover},
};
use crate::machine::link::status::Link;
use analyte_config::{DefinitionDetails, Endpoint, TransportKind};
use askama::Template as _;

pub(super) fn render_connection(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let (_, installed) = site::installed(state.setup.config(), id)?;
    let list = definitions(&state.setup.seams)?;
    let model = model_of(&list, draft, &installed)?;
    let document = site::read_document(&installed.site)?;
    let current_site = document.table();
    let lab_app = draft
        .lab_app
        .clone()
        .or_else(|| current_lab_app(state.setup.running(id), current_site));
    let lab_apps =
        match lab_apps::scan_with_site(state.setup.config(), Some((&installed.site, current_site)))
        {
            Ok(scan) => picker(&scan, lab_app.as_deref()),
            Err(error) => Picker::Unreadable(format::sentence(&error.to_string())),
        };
    let (how, inputs) = connection_inputs(state, id, &model.details, current_site, draft);
    let records_only = super::plan::recording::chosen(draft);
    Ok(ConnectionStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(model.name), Some(Step::Connection)),
        action: Step::Test.path(id),
        back: Step::Machine.path(id),
        hidden: draft.hidden_for_connection(),
        how,
        inputs,
        // Record only asks its own questions and sends to no lab app (ADR 0034).
        choices: if records_only {
            Vec::new()
        } else {
            choice_fields(&model.details, current_site, draft)
        },
        vidas_patient_namespace: (!records_only && model.details.asks_vidas_patient_namespace)
            .then(|| {
                draft
                    .vidas_patient_namespace
                    .clone()
                    .or_else(|| {
                        current_site
                            .get("vidas_patient_namespace")
                            .and_then(toml::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default()
            }),
        autolyser_poll_seconds: (!records_only && model.details.asks_autolyser_poll).then(|| {
            draft
                .poll_seconds
                .clone()
                .or_else(|| super::plan::saved_autolyser_poll(current_site))
                .unwrap_or_default()
        }),
        normal_poll_range: analyte_config::AUTOLYSER_POLL_SECONDS,
        recording_poll_range: analyte_config::recording::POLL_SECONDS,
        lab_apps,
        recording: records_only.then(|| recording_questions(&model.details, current_site, draft)),
        analyzer_mode: if records_only {
            analyzer_mode_field(&model.details, current_site, draft)
        } else {
            None
        },
    }
    .render())
}

/// Record only, step 2: the capture profile's questions for the model's
/// protocol (ADR 0034), filled in with the draft's or the saved answers.
fn recording_questions(
    definition: &DefinitionDetails,
    current_site: &toml::Table,
    draft: &Draft,
) -> page::Recording {
    let value = |given: &Option<String>, key: &str| {
        given
            .clone()
            .or_else(|| super::plan::recording::saved(current_site, key))
            .unwrap_or_default()
    };
    match definition.protocol.as_str() {
        "autolyser" => page::Recording::Autolyser {
            poll_seconds: value(&draft.poll_seconds, "poll_seconds"),
            controls: value(&draft.controls, "controls"),
        },
        "vidas" => page::Recording::Vidas {
            acknowledgment: value(&draft.acknowledgment, "acknowledgment"),
        },
        _ => page::Recording::Nothing,
    }
}

/// Record only, step 2: the analyzer mode the model's record file asks, from
/// its declared values; the draft's or the saved answer, else "Choose": the
/// installed mode is never assumed.
fn analyzer_mode_field(
    definition: &DefinitionDetails,
    current_site: &toml::Table,
    draft: &Draft,
) -> Option<Setting> {
    let mode = definition.analyzer_mode.as_ref()?;
    let current = draft
        .analyzer_mode
        .clone()
        .or_else(|| super::plan::recording::saved(current_site, "analyzer_mode"));
    let options: Vec<CableOption> = mode
        .values
        .iter()
        .map(|value| CableOption {
            selected: current.as_deref() == Some(value.value.as_str()),
            value: value.value.as_str().to_owned(),
            label: value.label.as_str().to_owned(),
        })
        .collect();
    Some(Setting {
        name: "analyzer_mode".to_owned(),
        id: "s2-analyzer-mode".to_owned(),
        label: mode.label.as_str().to_owned(),
        chosen: options.iter().any(|option| option.selected),
        hint: "As the analyzer shows it; it changes how the analyzer frames its records and cannot be guessed.".to_owned(),
        differs: false,
        options,
    })
}

/// The lab app picker: the known lab apps with `current` (or the only one) chosen.
fn picker(scan: &lab_apps::Scan, current: Option<&str>) -> Picker {
    if scan.known.is_empty() {
        return Picker::None;
    }
    let chosen = current
        .filter(|name| scan.get(name).is_some())
        .map(str::to_owned)
        .or_else(|| match scan.known.as_slice() {
            [only] => Some(only.settings.name.as_str().to_owned()),
            _ => None,
        });
    Picker::Choose {
        apps: scan
            .known
            .iter()
            .map(|known| LabApp {
                name: known.settings.name.as_str().to_owned(),
                address: known.settings.base_url.as_str().to_owned(),
                selected: chosen.as_deref() == Some(known.settings.name.as_str()),
            })
            .collect(),
        chosen: chosen.is_some(),
    }
}

/// How the model connects in one sentence, and the fields its transport asks for.
fn connection_inputs(
    state: &Context,
    id: &str,
    definition: &DefinitionDetails,
    current_site: &toml::Table,
    draft: &Draft,
) -> (&'static str, page::Inputs) {
    let current_endpoint = current_site
        .get("endpoint")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    match definition.kind {
        TransportKind::Serial => (
            "It connects by a serial cable or a USB serial adapter.",
            serial_choices(state, id, (definition, current_site), draft),
        ),
        kind @ (TransportKind::TcpServer | TransportKind::TcpClient) => {
            let (host, port) = super::plan::saved_tcp(current_endpoint).unwrap_or_default();
            let host = draft.host.as_deref().unwrap_or(host).to_owned();
            let port = draft.tcp_port.as_deref().unwrap_or(port).to_owned();
            if kind == TransportKind::TcpServer {
                (
                    concat!(
                        "The machine connects to this PC over the lab network. Choose the ",
                        "address Analyte listens on, then type that address and the port into ",
                        "the machine’s host (LIS) settings."
                    ),
                    page::Inputs::Listens {
                        hosts: (state.setup.seams.addresses)()
                            .map(|found| host_choices(&found, &host))
                            .map_err(|error| format::sentence(&error.to_string())),
                        typed: host,
                        port,
                        peers: draft
                            .allowed_peers
                            .clone()
                            .or_else(|| super::plan::saved_peers(current_site))
                            .unwrap_or_default(),
                    },
                )
            } else {
                (
                    concat!(
                        "Analyte connects to the machine over the lab network. Type the address ",
                        "and port shown in the machine’s own network or host (LIS) settings."
                    ),
                    page::Inputs::Connects { host, port },
                )
            }
        }
        TransportKind::WatchedFolder => (
            "Analyte reads its results from a folder.",
            page::Inputs::Folder(
                draft
                    .folder
                    .as_deref()
                    .unwrap_or(current_endpoint)
                    .to_owned(),
            ),
        ),
    }
}

/// The addresses Analyte can listen on: this PC's own, then every network
/// card, with `chosen` checked. A chosen address the list lacks (saved before,
/// or this PC only) comes first, so it stays visible and can be kept.
pub(in crate::admin::setup) fn host_choices(found: &[Address], chosen: &str) -> Vec<page::Host> {
    let chosen = chosen.trim();
    let every = std::net::Ipv4Addr::UNSPECIFIED.to_string();
    let mut hosts = Vec::with_capacity(found.len().saturating_add(2));
    let listed = chosen == every || found.iter().any(|a| a.ip.to_string() == chosen);
    if !chosen.is_empty() && !listed {
        let loopback = chosen
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
        hosts.push(page::Host {
            value: chosen.to_owned(),
            address: chosen.to_owned(),
            meta: if loopback {
                "this PC only: no machine on the network can reach it".into()
            } else {
                "saved before; not an address of this PC now".into()
            },
            checked: true,
        });
    }
    hosts.extend(found.iter().map(|address| {
        let ip = address.ip.to_string();
        page::Host {
            checked: ip == chosen,
            value: ip.clone(),
            address: ip,
            meta: address.adapter.clone(),
        }
    }));
    hosts.push(page::Host {
        checked: chosen == every,
        value: every.clone(),
        address: every,
        meta: "every network card of this PC".into(),
    });
    hosts
}

/// The detected ports with adapter name and serial number, then a typed name.
/// Without a choice in the draft, the port the site file names is chosen. A
/// port another running machine uses cannot be chosen.
fn serial_choices(
    state: &Context,
    id: &str,
    (definition, current_site): (&DefinitionDetails, &toml::Table),
    draft: &Draft,
) -> page::Inputs {
    let current = |key| current_site.get(key).and_then(toml::Value::as_str);
    let ports = (state.setup.seams.ports)()
        .map(|ports| {
            let listed: Vec<serial::Listed> =
                ports.iter().take(serial::LIST_LIMIT).cloned().collect();
            listed
                .iter()
                .map(|port| {
                    let (value, endpoint, saved) = match port.factory_serial() {
                        Some(number) => (
                            format!("usb:{number}"),
                            analyte_config::UsbSerial::new(number.into()).map_or_else(
                                |_| Endpoint::Fixed(port.name.clone()),
                                Endpoint::UsbSerial,
                            ),
                            format!("remembered by its serial number {number}"),
                        ),
                        None => (
                            format!("name:{}", port.name),
                            Endpoint::Fixed(port.name.clone()),
                            "no serial number; remembered by its port name".into(),
                        ),
                    };
                    let checked = match draft.port.as_deref() {
                        Some(chosen) => chosen == value,
                        None => match port.factory_serial() {
                            Some(number) => current("usb_serial") == Some(number),
                            None => current("endpoint") == Some(port.name.as_str()),
                        },
                    };
                    let used_by = holder(&state.setup, &endpoint, Some(&port.name), &|| {
                        Ok(listed.clone())
                    })
                    .filter(|holder| holder.as_str() != id)
                    .map(|holder| holder.as_str().to_owned());
                    Port {
                        meta: format!(
                            "{} – {}",
                            port.description.as_deref().unwrap_or("unknown adapter"),
                            used_by
                                .as_ref()
                                .map_or(saved, |holder| format!("used by {holder}"))
                        ),
                        value,
                        name: port.name.clone(),
                        checked: checked && used_by.is_none(),
                        used_by,
                    }
                })
                .collect()
        })
        .map_err(|error| format::sentence(&format!("listing serial ports failed: {error}")));
    page::Inputs::Serial {
        ports,
        typed: draft.port_name.clone().unwrap_or_default(),
        placeholder: current("endpoint").unwrap_or_default().to_owned(),
        cable: cable_fields(definition, current_site, draft),
    }
}

/// The cable settings of step 2 (ADR 0031), each from its standard list: the
/// draft's value, else the site file's saved choice, else the definition's
/// suggestion; without any, "Choose", so nothing is assumed.
fn cable_fields(
    definition: &DefinitionDetails,
    current_site: &toml::Table,
    draft: &Draft,
) -> Vec<Cable> {
    let suggestion = definition.serial.clone().unwrap_or_default();
    let saved = cable::saved(current_site).unwrap_or_default();
    analyte_config::SerialSetting::ALL
        .into_iter()
        .map(|setting| {
            let suggested = cable::value(setting, &suggestion);
            let current = cable::typed(draft, setting)
                .map(str::to_owned)
                .or_else(|| cable::value(setting, &saved))
                .or_else(|| suggested.clone());
            let options: Vec<CableOption> = cable::options(setting)
                .into_iter()
                .map(|(value, label)| CableOption {
                    selected: current.as_deref() == Some(value.as_str()),
                    value,
                    label,
                })
                .collect();
            let shown = |value: &str| {
                options
                    .iter()
                    .find(|option| option.value == value)
                    .map_or_else(|| value.to_owned(), |option| option.label.clone())
            };
            Cable {
                name: setting.key(),
                label: cable::label(setting),
                chosen: options.iter().any(|option| option.selected),
                hint: suggested.as_deref().map_or_else(
                    || {
                        "The machine’s definition suggests none: read it on the analyzer’s screen."
                            .to_owned()
                    },
                    |value| format!("The manual suggests {}.", shown(value)),
                ),
                differs: current.is_some() && current != suggested,
                options,
            }
        })
        .collect()
}

/// The model's own choices on step 2 (ADR 0032), each from its declared list:
/// the draft's value, else the site file's saved one, else the manual's
/// suggestion; without any, "Choose", so nothing is assumed.
fn choice_fields(
    definition: &DefinitionDetails,
    current_site: &toml::Table,
    draft: &Draft,
) -> Vec<Setting> {
    let saved = choices::saved(current_site);
    definition
        .choices
        .iter()
        .map(|choice| {
            let name = choice.name.as_str();
            let suggested = choice
                .suggested
                .as_ref()
                .map(|value| value.as_str().to_owned());
            let current = choices::typed(draft, name)
                .map(str::to_owned)
                .or_else(|| {
                    saved
                        .iter()
                        .find(|(saved, _)| saved == name)
                        .map(|(_, value)| value.clone())
                })
                .or_else(|| suggested.clone());
            let options: Vec<CableOption> = choice
                .values
                .iter()
                .map(|value| CableOption {
                    selected: current.as_deref() == Some(value.value.as_str()),
                    value: value.value.as_str().to_owned(),
                    label: value.label.as_str().to_owned(),
                })
                .collect();
            let hint = match choice
                .suggested
                .as_ref()
                .and_then(|value| choice.value(value))
            {
                Some(value) => format!(
                    "{} The manual suggests {}.",
                    choices::hint(choice.kind),
                    value.label.as_str()
                ),
                None => format!(
                    "{} The machine’s definition suggests none.",
                    choices::hint(choice.kind)
                ),
            };
            Setting {
                name: format!("{}{name}", choices::FIELD),
                id: format!("s2-choice-{name}"),
                label: choice.label.as_str().to_owned(),
                chosen: options.iter().any(|option| option.selected),
                differs: current.is_some() && current != suggested,
                hint,
                options,
            }
        })
        .collect()
}

/// The chosen connection in plain words, with the exact setting on hover.
pub(in crate::admin::setup) fn connection_words(link: Link, connection: &Endpoint) -> Hover {
    let shown = match (link, connection) {
        (_, Endpoint::UsbSerial(number)) => {
            format!("USB serial adapter with serial number {}", number.as_str())
        }
        (Link::Serial, Endpoint::Fixed(name)) => format!("Serial port {name}"),
        (Link::TcpServer, Endpoint::Fixed(address)) => {
            format!("Analyte listens on {address}; the machine connects to it")
        }
        (Link::TcpClient, Endpoint::Fixed(address)) => {
            format!("Analyte connects to the machine at {address}")
        }
        (Link::Folder, Endpoint::Fixed(folder)) => format!("Watched folder {folder}"),
    };
    Hover {
        shown,
        exact: site::describe(connection),
    }
}
