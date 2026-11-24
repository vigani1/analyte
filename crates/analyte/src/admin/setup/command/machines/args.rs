//! Reads machine command arguments and restores saved choices.
use super::running;
use crate::admin::setup::{
    Setup, SetupError, cable, choices, definitions,
    plan::{self, KEEP},
    site,
    wizard::Draft,
};
use analyte_config::{SerialSetting, TransportKind};

/// A serial machine's cable settings, as step 2 chooses them (ADR 0031):
/// match the analyzer's LIS/host settings screen. A setting not given keeps
/// its saved value, else the model's suggestion.
#[derive(clap::Args, Clone, Default)]
pub struct Cable {
    /// Serial cable: the baud rate: 300, 600, 1200, 2400, 4800, 9600, 19200,
    /// 38400, 57600 or 115200.
    #[arg(long, value_name = "RATE")]
    baud: Option<String>,
    /// Serial cable: data bits, 5 to 8.
    #[arg(long, value_name = "BITS")]
    data_bits: Option<String>,
    /// Serial cable: parity: none, odd or even.
    #[arg(long, value_name = "PARITY")]
    parity: Option<String>,
    /// Serial cable: stop bits, 1 or 2.
    #[arg(long, value_name = "BITS")]
    stop_bits: Option<String>,
    /// Serial cable: flow control: none, hardware (RTS/CTS) or software (XON/XOFF).
    #[arg(long, value_name = "FLOW")]
    flow_control: Option<String>,
}

impl Cable {
    /// `draft` with each given setting applied.
    fn over(self, mut draft: Draft) -> Draft {
        for (setting, given) in [
            (SerialSetting::Baud, self.baud),
            (SerialSetting::DataBits, self.data_bits),
            (SerialSetting::Parity, self.parity),
            (SerialSetting::StopBits, self.stop_bits),
            (SerialSetting::FlowControl, self.flow_control),
        ] {
            if given.is_some() {
                cable::set(&mut draft, setting, given);
            }
        }
        draft
    }

    /// The settings given, read and checked as step 2 reads them; none when
    /// none is given.
    pub(super) fn chosen(self) -> Result<Option<analyte_config::SerialValues>, SetupError> {
        let draft = self.over(Draft::default());
        if !cable::given(&draft) {
            return Ok(None);
        }
        cable::chosen(&draft, None).map(Some)
    }
}

/// A machine's model, connection and lab app, as step 1 and step 2 choose them.
#[derive(clap::Args, Clone, Default)]
pub struct Choice {
    /// The machine model: its key as `machines models` lists it, or
    /// `installed` to keep the current one.
    #[arg(long)]
    model: Option<String>,
    /// A serial port by name, such as COM3.
    #[arg(long, group = "connection")]
    serial_port: Option<String>,
    /// A USB serial adapter by its serial number, so it is found in any USB socket.
    #[arg(long, group = "connection")]
    usb_serial: Option<String>,
    /// The IP address of a network link (this PC's when the machine connects
    /// to Analyte, the machine's when Analyte connects to it), with --port.
    #[arg(long, group = "connection", requires = "port")]
    address: Option<String>,
    /// The network port number, with --address.
    #[arg(long, requires = "address")]
    port: Option<String>,
    /// The folder Analyte reads results from.
    #[arg(long, group = "connection")]
    folder: Option<String>,
    /// The lab app this machine sends results to.
    #[arg(long)]
    lab_app: Option<String>,
    /// For a machine that connects to Analyte: accept connections only from
    /// this device, the analyzer's own IP address; repeat for several. Any
    /// other device is refused (ADR 0026).
    #[arg(
        long = "allowed-peer",
        value_name = "IP",
        conflicts_with = "any_device"
    )]
    allowed_peer: Vec<String>,
    /// For a machine that connects to Analyte: accept connections from any
    /// device again, removing its allowed devices.
    #[arg(long)]
    any_device: bool,
    /// The serial cable settings.
    #[command(flatten)]
    cable: Cable,
    /// One of the model's own settings, as step 2 lists them under Analyzer
    /// settings and `machines models` names them; repeat for several. A
    /// setting not given keeps its saved value, else the model's suggestion.
    #[arg(long = "choice", value_name = "NAME=VALUE", value_parser = choice_pair)]
    choice: Vec<(String, String)>,
    /// Record only (ADR 0034): keep the analyzer talking and record every
    /// byte, for writing its definition; nothing is sent to a lab app, so no
    /// --lab-app. Also for a model not ready yet. Recordings contain patient
    /// data: export them with `analyte captures export`.
    #[arg(long, conflicts_with_all = ["lab_app", "connect_normally"])]
    record_only: bool,
    /// Leaves record only: connect the machine normally again (needs a ready
    /// model and --lab-app). Recorded messages are never delivered.
    #[arg(long)]
    connect_normally: bool,
    /// Autolyser: seconds between report polls, when the model or capture profile asks.
    #[arg(long, value_name = "SECONDS")]
    poll_seconds: Option<String>,
    /// Record only, Autolyser: whether its reports carry controls: yes or no.
    #[arg(long, value_name = "YES_OR_NO")]
    reports_carry_controls: Option<String>,
    /// Record only, VIDAS: the VIDAS PC acknowledgment: literal or alternate.
    #[arg(long, value_name = "MODE")]
    vidas_acknowledgment: Option<String>,
    /// Record only, for a model that asks it (cobas e 411: elecsys or cobas):
    /// the mode the analyzer's own settings show; `machines models` lists it.
    #[arg(long, value_name = "VALUE")]
    analyzer_mode: Option<String>,
}

/// Reads `--choice NAME=VALUE`; the model's own checks follow.
fn choice_pair(text: &str) -> Result<(String, String), String> {
    text.split_once('=')
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, value)| !name.is_empty() && !value.is_empty())
        .ok_or_else(|| "write NAME=VALUE, such as protocol_mode=alternate".to_owned())
}

impl Choice {
    /// `draft` with these choices applied; a new connection replaces the saved one.
    pub(super) fn over(self, mut draft: Draft) -> Draft {
        let serial = self.serial_port.is_some() || self.usb_serial.is_some();
        if serial || self.address.is_some() || self.folder.is_some() {
            // The analyzer is the same device whichever address Analyte listens
            // on, and the same cable whichever port it is plugged into.
            let kept = Draft {
                definition: draft.definition.take(),
                lab_app: draft.lab_app.take(),
                allowed_peers: draft.allowed_peers.take(),
                mode: draft.mode.take(),
                poll_seconds: draft.poll_seconds.take(),
                controls: draft.controls.take(),
                acknowledgment: draft.acknowledgment.take(),
                analyzer_mode: draft.analyzer_mode.take(),
                ..Draft::default()
            };
            draft = if serial {
                Draft {
                    baud: draft.baud,
                    data_bits: draft.data_bits,
                    parity: draft.parity,
                    stop_bits: draft.stop_bits,
                    flow_control: draft.flow_control,
                    ..kept
                }
            } else {
                kept
            };
        }
        draft = self.cable.over(draft);
        draft.choices.extend(self.choice);
        if !self.allowed_peer.is_empty() {
            draft.allowed_peers = Some(self.allowed_peer.join(", "));
        }
        if self.any_device {
            // Chosen: none, so nothing is asked about it either.
            draft.allowed_peers = Some(String::new());
        }
        if let Some(model) = self.model {
            draft.definition = Some(model);
        }
        draft.port_name = self.serial_port.or(draft.port_name);
        if let Some(number) = self.usb_serial {
            draft.port = Some(format!("usb:{number}"));
        }
        draft.host = self.address.or(draft.host);
        draft.tcp_port = self.port.or(draft.tcp_port);
        draft.folder = self.folder.or(draft.folder);
        if let Some(lab_app) = self.lab_app {
            draft.lab_app = Some(lab_app);
        }
        let was_recording = plan::recording::chosen(&draft);
        if self.record_only {
            draft.mode = Some(plan::recording::RECORD.to_owned());
            draft.lab_app = None;
        }
        if self.connect_normally {
            draft.mode = None;
        }
        if was_recording != plan::recording::chosen(&draft) {
            // Normal operation and capture use separate site answers and bounds.
            draft.poll_seconds = None;
        }
        draft.poll_seconds = self.poll_seconds.or(draft.poll_seconds);
        draft.controls = self.reports_carry_controls.or(draft.controls);
        draft.acknowledgment = self.vidas_acknowledgment.or(draft.acknowledgment);
        draft.analyzer_mode = self.analyzer_mode.or(draft.analyzer_mode);
        draft
    }
}

/// The machine's saved choices as a draft: its model kept, its connection and
/// its lab app, as step 2 shows them before anything is changed.
pub(super) fn saved(setup: &Setup, id: &str) -> Result<Draft, SetupError> {
    let (_, installed) = site::installed(setup.config(), id)?;
    let table = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let text = |key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };
    let answer = |key: &str| plan::recording::saved(&table, key);
    let mut draft = Draft {
        definition: installed.definition.as_ref().map(|_| KEEP.to_owned()),
        lab_app: plan::current_lab_app(running(setup, id), &table),
        allowed_peers: plan::saved_peers(&table),
        // A record-only machine stays record-only until --connect-normally (ADR 0034).
        mode: table
            .contains_key("recording")
            .then(|| plan::recording::RECORD.to_owned()),
        poll_seconds: answer("poll_seconds"),
        controls: answer("controls"),
        acknowledgment: answer("acknowledgment"),
        analyzer_mode: answer("analyzer_mode"),
        ..Draft::default()
    };
    let kind = installed.definition.as_deref().and_then(plan::kind_of);
    // Also before a model is chosen: `machines add` may have kept cable settings.
    if matches!(kind, Some(TransportKind::Serial) | None) {
        cable::keep_saved(&mut draft, &table);
    }
    match (kind, text("usb_serial"), text("endpoint")) {
        (_, Some(number), _) => draft.port = Some(format!("usb:{number}")),
        (Some(TransportKind::Serial), None, endpoint) => draft.port_name = endpoint,
        (Some(TransportKind::TcpServer | TransportKind::TcpClient), None, Some(endpoint)) => {
            if let Some((host, port)) = plan::saved_tcp(&endpoint) {
                draft.host = Some(host.to_owned());
                draft.tcp_port = Some(port.to_owned());
            }
        }
        (Some(TransportKind::TcpServer | TransportKind::TcpClient), None, None) => {}
        (Some(TransportKind::WatchedFolder) | None, None, endpoint) => draft.folder = endpoint,
    }
    Ok(draft)
}

/// Fills each of the draft model's own choices not given with `--choice`
/// from the saved site file, as step 2 starts from them; a saved choice the
/// model does not declare (another model's) is left behind. Nothing is
/// filled while the model is unknown; the plan refuses that by itself.
pub(super) fn keep_saved_choices(
    setup: &Setup,
    id: &str,
    draft: &mut Draft,
) -> Result<(), SetupError> {
    let (_, installed) = site::installed(setup.config(), id)?;
    let list = definitions(&setup.seams)?;
    let Ok(model) = plan::model_of(&list, draft, &installed) else {
        return Ok(());
    };
    let table = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    choices::keep_saved(draft, &table, &choices::declared(&model.path));
    if !plan::recording::chosen(draft)
        && plan::asks_autolyser_poll(&model.path)
        && draft.poll_seconds.is_none()
    {
        draft.poll_seconds = plan::saved_autolyser_poll(&table);
    }
    Ok(())
}
