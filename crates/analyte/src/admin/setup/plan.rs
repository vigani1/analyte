//! Plans a machine's setup from the wizard's choices: the model, the connection and the lab app, checked against the files.
//!
//! Every step and the save build the same [`Plan`] in memory first; only
//! [`save_blocking`] writes, under the write lock. The connection test finds its
//! target here too, refusing a port or address a running machine uses.
use super::{
    Definition, Running, SetupError, Shipped, definitions, lab_apps, listen, site, wizard,
};
use crate::admin::server::Context;
use crate::installation::config::{Program, connections};
use crate::machine::link::status::Link;
use analyte_config::{Endpoint, TransportKind};
use analyte_model::Identifier;
use analyte_transport::serial;
use serde::Deserialize;
use std::path::PathBuf;

/// A definition's transport kind, used only to choose which inputs to show;
/// `None` when it cannot be read or names no known kind.
pub(super) fn kind_of(path: &std::path::Path) -> Option<TransportKind> {
    let text = crate::installation::config::read(path).ok()?;
    let table: toml::Table = text.parse().ok()?;
    table.get("transport")?.get("kind")?.clone().try_into().ok()
}

/// The form value that keeps the installed model when it is not a shipped one.
pub(super) const KEEP: &str = "installed";

/// The machine model a setup step uses.
pub(super) struct Model {
    /// The shipped model's `vendor/model` key; `None` for an installed model
    /// kept from outside the shipped list.
    pub(super) key: Option<String>,
    pub(super) path: PathBuf,
    /// The name for people; a kept model shows its file.
    pub(super) name: String,
}

impl Model {
    /// The model as the step forms carry it: its key, or [`KEEP`].
    pub(super) fn form_value(&self) -> &str {
        self.key.as_deref().unwrap_or(KEEP)
    }
}

/// Finds the chosen shipped definition, or the installed one when none is chosen.
pub(super) fn chosen(
    list: &[Definition],
    key: Option<&str>,
    installed: &site::Installed,
) -> Result<Model, SetupError> {
    let found = match key {
        Some(key) => list
            .iter()
            .find(|d| d.key == key)
            .ok_or(SetupError::UnknownDefinition)?,
        None => {
            let installed = installed
                .definition
                .as_ref()
                .ok_or(SetupError::NoDefinitionYet)?;
            let current = installed.canonicalize().ok();
            match list.iter().find(|d| match &d.state {
                Shipped::Ready { path, .. } => path.canonicalize().ok() == current,
                Shipped::Blocked | Shipped::Unreadable(_) => false,
            }) {
                Some(found) => found,
                None => {
                    return Ok(Model {
                        key: None,
                        path: installed.clone(),
                        name: installed.display().to_string(),
                    });
                }
            }
        }
    };
    match &found.state {
        Shipped::Ready { path, .. } => Ok(Model {
            key: Some(found.key.clone()),
            path: path.clone(),
            name: found.display.clone(),
        }),
        Shipped::Blocked | Shipped::Unreadable(_) => {
            Err(SetupError::BlockedDefinition(found.key.clone()))
        }
    }
}

/// The lab app the machine sends to now: the running one's, or for a machine
/// not running yet, the one its site file names or writes.
pub(super) fn current_lab_app(
    running: Option<&Running>,
    current_site: &toml::Table,
) -> Option<String> {
    if let Some(running) = running {
        return Some(running.lab_app.as_str().to_owned());
    }
    current_site
        .get("lab_app")
        .or_else(|| current_site.get("destination").and_then(|d| d.get("name")))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
}

/// A setup step posted for the test or the save: the CSRF value and the
/// [`wizard::Draft`] fields. It holds no key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Choice {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    pub(super) definition: Option<String>,
    pub(super) port: Option<String>,
    pub(super) port_name: Option<String>,
    pub(super) host: Option<String>,
    pub(super) tcp_port: Option<String>,
    pub(super) folder: Option<String>,
    pub(super) lab_app: Option<String>,
}

/// The connection the draft names for a definition of transport `kind`; an
/// unknown kind asks for a folder, as a watched folder does.
fn connection_of(
    kind: Option<TransportKind>,
    choice: &wizard::Draft,
) -> Result<Endpoint, SetupError> {
    let field = |value: &Option<String>| value.as_deref().unwrap_or_default().trim().to_owned();
    match kind {
        Some(TransportKind::Serial) => {
            let typed = field(&choice.port_name);
            if !typed.is_empty() {
                return Ok(Endpoint::Fixed(typed));
            }
            match choice.port.as_deref().and_then(|v| v.split_once(':')) {
                Some(("usb", number)) => Ok(Endpoint::UsbSerial(
                    analyte_config::UsbSerial::new(number.into())
                        .map_err(SetupError::Connection)?,
                )),
                Some(("name", name)) if !name.is_empty() => Ok(Endpoint::Fixed(name.into())),
                _ => Err(SetupError::Connection("choose a serial port")),
            }
        }
        Some(TransportKind::TcpServer | TransportKind::TcpClient) => {
            let host: std::net::IpAddr = field(&choice.host)
                .parse()
                .map_err(|_| SetupError::Connection("enter an IP address such as 192.168.1.20"))?;
            let port: u16 = field(&choice.tcp_port)
                .parse()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(SetupError::Connection(
                    "enter a port number from 1 to 65535",
                ))?;
            Ok(Endpoint::Fixed(
                std::net::SocketAddr::new(host, port).to_string(),
            ))
        }
        Some(TransportKind::WatchedFolder) | None => {
            let folder = field(&choice.folder);
            if folder.is_empty() {
                return Err(SetupError::Connection("enter the folder path"));
            }
            Ok(Endpoint::Fixed(folder))
        }
    }
}

/// Everything a save or a test needs, checked and validated in memory.
pub(super) struct Plan {
    pub(super) program: String,
    pub(super) installed: site::Installed,
    pub(super) definition: PathBuf,
    /// The model's name for people.
    pub(super) model: String,
    pub(super) connection: Endpoint,
    pub(super) site_text: String,
    /// The new `lab-apps.toml` text, when the save moves a lab app into it.
    pub(super) lab_apps: Option<String>,
    pub(super) validated: analyte_config::Validated,
}

/// Checks every choice of `draft` against the files, in step order, and
/// validates the result as the program loader does; writes nothing.
pub(super) fn plan(state: &Context, id: &str, draft: &wizard::Draft) -> Result<Plan, SetupError> {
    let (program, installed) = site::installed(&state.config_path, id)?;
    let list = definitions(state)?;
    let Model {
        path: definition,
        name: model,
        ..
    } = model_of(&list, draft, &installed)?;
    let connection = connection_of(kind_of(&definition), draft)?;
    let lab_app = Identifier::new(draft.lab_app.as_deref().unwrap_or_default().trim())
        .map_err(|_| SetupError::NoLabAppChosen)?;
    let current = crate::installation::config::read(&installed.site)?;
    let scan = lab_apps::scan(&state.config_path)?;
    let candidate = site::candidate(
        &current,
        &connection,
        &lab_app,
        &scan.file,
        &scan.settings(),
    )?;
    let validated = site::validate(
        &installed,
        &definition,
        &candidate.text,
        &candidate.lab_apps,
    )?;
    let lab_apps = if candidate.lab_apps == scan.file {
        None
    } else {
        Some(candidate.lab_apps.text()?)
    };
    Ok(Plan {
        program,
        installed,
        definition,
        model,
        connection,
        site_text: candidate.text,
        lab_apps,
        validated,
    })
}

pub(super) struct Saved {
    pub(super) digest: [u8; 32],
    pub(super) model: String,
    pub(super) link: Link,
    pub(super) connection: Endpoint,
    pub(super) lab_app: String,
    pub(super) base_url: String,
    /// The save moved a lab app into `lab-apps.toml`.
    pub(super) moved: bool,
    pub(super) backups: Vec<PathBuf>,
}

pub(super) fn save_blocking(
    state: &Context,
    id: &str,
    draft: &wizard::Draft,
) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let plan = plan(state, id, draft)?;
    shares_connection(state, id, &plan)?;
    let moved = plan.lab_apps.is_some();
    let written = site::apply(
        &state.config_path,
        &plan.program,
        &plan.installed,
        &plan.definition,
        &plan.site_text,
        plan.lab_apps,
    )?;
    let destination = &plan.validated.effective().site().destination;
    Ok(Saved {
        digest: plan.validated.digest(),
        model: plan.model,
        link: Link::of(plan.validated.effective().stack()),
        connection: plan.connection,
        lab_app: destination.name.as_str().to_owned(),
        base_url: destination.base_url.as_str().to_owned(),
        moved,
        backups: written.backups,
    })
}

/// The model the draft chose; [`KEEP`] keeps the current one.
pub(super) fn model_of(
    list: &[Definition],
    draft: &wizard::Draft,
    installed: &site::Installed,
) -> Result<Model, SetupError> {
    match draft.definition.as_deref().map(str::trim) {
        None | Some("") => Err(SetupError::UnknownDefinition),
        Some(KEEP) => chosen(list, None, installed),
        Some(key) => chosen(list, Some(key), installed),
    }
}

/// Refuses a test on a serial port or network address a running machine uses.
/// A running machine named by `usb_serial` is looked up in the current port list,
/// so choosing its port by name is refused too.
fn in_use(state: &Context, chosen: &Endpoint, port: Option<&str>) -> Result<(), SetupError> {
    for running in &state.setup.running {
        let same = if let Some(port) = port {
            running.link == Link::Serial && {
                let running_port = match &running.endpoint {
                    Endpoint::Fixed(name) => Some(name.clone()),
                    Endpoint::UsbSerial(number) => {
                        serial::locate(&serial::Device::UsbSerial(number.as_str().into()), || {
                            (state.setup.seams.ports)()
                        })
                        .ok()
                    }
                };
                let same_number = matches!(
                    (&running.endpoint, chosen),
                    (Endpoint::UsbSerial(left), Endpoint::UsbSerial(right))
                        if left.as_str().eq_ignore_ascii_case(right.as_str())
                );
                same_number
                    || running_port.is_some_and(|name| connections::same_port_name(&name, port))
            }
        } else {
            matches!(running.link, Link::TcpServer | Link::TcpClient)
                && connections::listeners_overlap(&running.endpoint, chosen)
        };
        if same {
            return Err(SetupError::InUse(running.id.as_str().into()));
        }
    }
    Ok(())
}

/// Refuses, before writing, a connection another machine in the program file
/// already uses. When the current program does not load, the reload after
/// writing refuses instead, so a broken file can still be fixed here.
fn shares_connection(state: &Context, id: &str, plan: &Plan) -> Result<(), SetupError> {
    let Ok(program) = Program::load(&state.config_path) else {
        return Ok(());
    };
    let chosen = (
        Link::of(plan.validated.effective().stack()),
        &plan.connection,
    );
    for other in program.machines().iter().filter(|m| m.id().as_str() != id) {
        if let Some(what) = connections::shared(connections::of(other), chosen) {
            return Err(crate::installation::config::Error::SharedConnection {
                first: other.id().as_str().into(),
                second: id.into(),
                what,
            }
            .into());
        }
    }
    Ok(())
}

pub(super) fn listen_target(
    state: &Context,
    plan: &Plan,
) -> Result<(String, listen::Target), SetupError> {
    let stack = plan.validated.effective().stack();
    let endpoint = &plan.connection;
    match Link::of(stack) {
        Link::Serial => {
            let device = match endpoint {
                Endpoint::Fixed(name) => serial::Device::Named(name.clone()),
                Endpoint::UsbSerial(number) => serial::Device::UsbSerial(number.as_str().into()),
            };
            let ports = state.setup.seams.ports.clone();
            let name = serial::locate(&device, || ports())?;
            in_use(state, endpoint, Some(&name))?;
            let settings = crate::machine::link::serial::line_settings(
                stack,
                serial::Device::Named(name.clone()),
                listen::SERIAL_READ,
            )
            .ok_or(SetupError::NothingToListen)?
            .map_err(SetupError::SerialSettings)?;
            Ok((
                format!("serial port {name}"),
                listen::Target::Serial {
                    name,
                    settings,
                    open: state.setup.seams.open.clone(),
                },
            ))
        }
        link @ (Link::TcpServer | Link::TcpClient) => {
            in_use(state, endpoint, None)?;
            let Endpoint::Fixed(text) = endpoint else {
                return Err(SetupError::Connection(
                    "a network link needs an IP address and port",
                ));
            };
            let address = text
                .parse()
                .map_err(|_| SetupError::Connection("enter an IP address and port"))?;
            Ok(if link == Link::TcpServer {
                (
                    format!("listening on {address}"),
                    listen::Target::TcpServer(address),
                )
            } else {
                (
                    format!("connecting to {address}"),
                    listen::Target::TcpClient(address),
                )
            })
        }
        Link::Folder => Err(SetupError::NothingToListen),
    }
}
