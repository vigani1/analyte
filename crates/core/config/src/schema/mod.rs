//! Holds raw TOML configuration types.
pub(crate) mod law;
pub use law::{Law, LawConnectionMode, LawPeer, LawQueryForm, LawWork, PeerSupport};
mod vidas;
pub use vidas::{
    AckPoint, ChecksumAcceptance, ChecksumInclusion, HexCase,
    ResultControlLabels as VidasResultControlLabels, Settings as Vidas,
};

use analyte_model::Identifier;
use serde::{Deserialize, Serialize};
mod mappings;
pub use mappings::{
    AstmTestComponent, ComponentParts, Field, IdentityEvidence, IdentityReference, Mappings,
    NoSubId, OrderNamespace, QcRule, SampleSelection, SubId, UnitMapping, ValueExpectation,
};
mod poct;
pub use poct::Poct;
mod autolyser;
pub use autolyser::Autolyser;
pub(crate) use autolyser::PollInterval;
mod departures;
pub use departures::{
    DepartureDeclaration, DepartureDirection, DepartureEdition, DepartureGuarantee,
    DepartureMessage, DepartureReference, DepartureScope, MachineDeparture,
};
mod refusal;
pub use refusal::DepartureRefusal;
mod choices;
pub mod software_version;
pub use choices::{
    ChoiceBlock, ChoiceKind, ChoiceLabel, ChoiceName, ChoiceSelections, ChoiceValue,
    MAX_CHOICE_COMBINATIONS, MAX_CHOICE_VALUES, MAX_CHOICES, MachineChoice, OrdersSwitch,
    VidasChoice,
};

/// Declared evidence; loading never promotes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    /// Built from documents only; no simulator or hardware evidence claimed.
    Documented,
    /// Backed by simulator or vendor-trace evidence, without a hardware claim.
    Simulated,
    /// Backed by recorded conversations with the real instrument.
    Proven,
}

/// Physical connection role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Opens a host-owned serial port.
    Serial,
    /// Connects to the configured peer TCP endpoint.
    TcpClient,
    /// Accepts inbound TCP connections on the configured listener.
    TcpServer,
    /// Receives files from a host-owned watched directory.
    WatchedFolder,
}

/// Serial parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    /// Disables this serial line option.
    None,
    /// Requires an odd parity bit count.
    Odd,
    /// Requires an even parity bit count.
    Even,
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowControl {
    /// Disables this serial line option.
    None,
    /// Uses hardware handshake lines for flow control.
    Hardware,
    /// Uses XON/XOFF bytes for flow control.
    Software,
}

impl From<Parity> for analyte_transport::serial::Parity {
    fn from(parity: Parity) -> Self {
        match parity {
            Parity::None => Self::None,
            Parity::Odd => Self::Odd,
            Parity::Even => Self::Even,
        }
    }
}

impl From<FlowControl> for analyte_transport::serial::FlowControl {
    fn from(flow: FlowControl) -> Self {
        match flow {
            FlowControl::None => Self::None,
            FlowControl::Hardware => Self::Hardware,
            FlowControl::Software => Self::Software,
        }
    }
}

/// LIS01 checksum compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumHex {
    /// Accepts only the standard uppercase checksum spelling.
    Strict,
    /// Also accepts lowercase checksum hex digits.
    AcceptLowercase,
}

/// Side that wins simultaneous ENQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Gives the instrument priority during simultaneous ENQ.
    Instrument,
    /// Gives the host priority during simultaneous ENQ.
    Host,
}

/// HL7 acknowledgment promise, ADR 0003.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Responsibility {
    /// Promises durable custody before acknowledging the transfer.
    StoreAndForward,
}

/// Strict text decoding; invalid bytes must be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Rejects text bytes outside ASCII.
    Ascii,
    /// Decodes each byte using ISO-8859-1.
    Latin1,
    /// Requires well-formed UTF-8 text.
    Utf8,
}

/// Negotiated segment ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminator {
    /// Ends segments with carriage return.
    Cr,
    /// Ends segments with carriage return followed by line feed.
    CrLf,
    /// Ends segments with line feed.
    Lf,
}

/// Malformed escape handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escapes {
    /// Refuses malformed escape sequences while retaining their bytes.
    Reject,
    /// Keeps malformed escape text for explicit downstream handling.
    Preserve,
}

/// A config spelling of the protocol-owned HL7 edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Edition(analyte_hl7::Version);

impl TryFrom<String> for Edition {
    type Error = String;
    fn try_from(key: String) -> Result<Self, Self::Error> {
        analyte_hl7::Version::from_config_key(&key)
            .map(Self)
            .ok_or_else(|| {
                format!(
                    "unknown variant `{key}`, expected one of {}",
                    analyte_hl7::Version::config_keys()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}
impl From<analyte_hl7::Version> for Edition {
    fn from(version: analyte_hl7::Version) -> Self {
        Self(version)
    }
}
impl From<Edition> for String {
    fn from(edition: Edition) -> Self {
        edition.0.config_key().into()
    }
}
impl From<Edition> for analyte_hl7::Version {
    fn from(edition: Edition) -> Self {
        edition.0
    }
}

/// How the machine obtained the sample identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleSource {
    /// Sample identity comes from the instrument barcode reader.
    Barcode,
    /// Sample identity comes from the supplied host order.
    HostOrder,
    /// Sample identity was entered by an operator.
    Operator,
    /// Makes no claim about the source or specimen role.
    Unknown,
}

/// Field ownership; selections never search globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Applies the configured rule to the whole message.
    Message,
    /// Applies the configured patient scope or patient specimen role.
    Patient,
    /// Applies the configured rule to an order group.
    Order,
    /// Applies the configured rule to one observation.
    Observation,
}

/// Classification evidence, never a QC decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Applies the configured patient scope or patient specimen role.
    Patient,
    /// Identifies a control specimen without making a QC decision.
    Control,
    /// Identifies a calibration specimen without interpreting it.
    Calibration,
    /// Makes no claim about the source or specimen role.
    Unknown,
}

/// Complete serial line settings, built only by validation from the layers'
/// [`SerialValues`]; the endpoint belongs to the site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Serial {
    /// Bits per second, one of [`analyte_transport::serial::BAUD_RATES`].
    pub baud: u32,
    /// Data bits, five through eight.
    pub data_bits: u8,
    /// Parity check.
    pub parity: Parity,
    /// One or two stop bits.
    pub stop_bits: u8,
    /// Flow control.
    pub flow_control: FlowControl,
}

/// One of the five serial cable settings, named as the TOML keys name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialSetting {
    /// `baud`.
    Baud,
    /// `data_bits`.
    DataBits,
    /// `parity`.
    Parity,
    /// `stop_bits`.
    StopBits,
    /// `flow_control`.
    FlowControl,
}

impl SerialSetting {
    /// Every setting, in the order the pages and commands show them.
    pub const ALL: [Self; 5] = [
        Self::Baud,
        Self::DataBits,
        Self::Parity,
        Self::StopBits,
        Self::FlowControl,
    ];

    /// The TOML key under `[transport.serial]` and the site's `[serial]`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Baud => "baud",
            Self::DataBits => "data_bits",
            Self::Parity => "parity",
            Self::StopBits => "stop_bits",
            Self::FlowControl => "flow_control",
        }
    }
}

impl std::fmt::Display for SerialSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

/// Serial cable settings as one layer writes them; any value may be absent.
/// The definition's `[transport.serial]` holds the manual's suggestion and the
/// site file's `[serial]` the lab's choice, which wins value by value (ADR 0031).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerialValues {
    /// Bits per second, one of [`analyte_transport::serial::BAUD_RATES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baud: Option<u32>,
    /// Data bits, five through eight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_bits: Option<u8>,
    /// Parity check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parity: Option<Parity>,
    /// One or two stop bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_bits: Option<u8>,
    /// Flow control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_control: Option<FlowControl>,
}

impl SerialValues {
    /// These values, each absent one taken from `base`.
    #[must_use]
    pub fn over(&self, base: Option<&Self>) -> Self {
        let base = base.cloned().unwrap_or_default();
        Self {
            baud: self.baud.or(base.baud),
            data_bits: self.data_bits.or(base.data_bits),
            parity: self.parity.or(base.parity),
            stop_bits: self.stop_bits.or(base.stop_bits),
            flow_control: self.flow_control.or(base.flow_control),
        }
    }

    /// The complete settings, or the first setting that has no value; no
    /// value is ever assumed.
    ///
    /// # Errors
    /// Names the first missing setting.
    pub fn complete(&self) -> Result<Serial, SerialSetting> {
        Ok(Serial {
            baud: self.baud.ok_or(SerialSetting::Baud)?,
            data_bits: self.data_bits.ok_or(SerialSetting::DataBits)?,
            parity: self.parity.ok_or(SerialSetting::Parity)?,
            stop_bits: self.stop_bits.ok_or(SerialSetting::StopBits)?,
            flow_control: self.flow_control.ok_or(SerialSetting::FlowControl)?,
        })
    }
}

impl Serial {
    /// The settings that differ from `suggestion`, in [`SerialSetting::ALL`]
    /// order; a setting the suggestion leaves out counts as differing.
    #[must_use]
    pub fn differs_from(&self, suggestion: Option<&SerialValues>) -> Vec<SerialSetting> {
        let suggested = suggestion.cloned().unwrap_or_default();
        SerialSetting::ALL
            .into_iter()
            .filter(|setting| match setting {
                SerialSetting::Baud => suggested.baud != Some(self.baud),
                SerialSetting::DataBits => suggested.data_bits != Some(self.data_bits),
                SerialSetting::Parity => suggested.parity != Some(self.parity),
                SerialSetting::StopBits => suggested.stop_bits != Some(self.stop_bits),
                SerialSetting::FlowControl => suggested.flow_control != Some(self.flow_control),
            })
            .collect()
    }
}

/// Draft connection settings, checked before activation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transport {
    /// Connection role.
    pub kind: TransportKind,
    /// Only for serial: the values this layer writes; validation completes them.
    pub serial: Option<SerialValues>,
    /// Deadline for external operations.
    pub operation_timeout_ms: u32,
}

/// Explicit wire framing bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    /// Frame bytes including ASTM overhead, or payload bytes otherwise.
    pub limit: usize,
    /// Required only for LIS01.
    pub checksum_hex: Option<ChecksumHex>,
}

/// Dialog settings consumed by the host state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dialog {
    /// Receive deadline.
    pub receive_timeout_ms: u32,
    /// Send deadline.
    pub send_timeout_ms: u32,
    /// Required only for ASTM.
    pub contention_priority: Option<Priority>,
    /// Required only for HL7.
    pub acknowledgment: Option<Responsibility>,
    /// Optional 2.9.1 capability request; true is refused until a separate channel is supported.
    pub enhanced_application_acknowledgments: Option<bool>,
}

/// Syntax options checked against the selected protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Syntax {
    /// Negotiated encoding.
    pub encoding: Encoding,
    /// Required only for HL7.
    pub edition: Option<Edition>,
    /// Required only for HL7.
    pub terminator: Option<Terminator>,
    /// Required only for HL7.
    pub escapes: Option<Escapes>,
    /// Named vendor departure for the later UTF-8 code in HL7 2.3.1.
    #[serde(default)]
    pub allow_utf8_in_v231: bool,
    /// Total message bytes.
    pub bytes: usize,
    /// Maximum records or segments.
    pub records: usize,
    /// Maximum ASTM record bytes.
    pub record_bytes: usize,
    /// Maximum fields.
    pub fields: usize,
    /// Maximum repetitions.
    pub repetitions: usize,
    /// Maximum ASTM components.
    pub components: usize,
}

/// Evidence applies only to these exercised capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    /// Analyzer model.
    pub model: Identifier,
    /// Firmware or explicit unknown.
    pub firmware: Identifier,
    /// Driver version.
    pub driver: Identifier,
    /// Digest of the tested config, or untested.
    pub config_digest: Identifier,
    /// Exercised transport.
    pub transport: TransportKind,
    /// Tested dialogs.
    pub dialogs: Vec<Identifier>,
    /// Exercised failures.
    pub failure_cases: Vec<Identifier>,
    /// Platform or untested.
    pub platform: Identifier,
    /// Evidence file references.
    pub evidence_files: Vec<Identifier>,
}

/// Declared summary and its scope, ADR 0005.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Declared level.
    pub level: EvidenceLevel,
    /// Capability evidence.
    pub capabilities: Vec<Capability>,
}

/// Shipped definition contains protocol and mapping rules only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineDocument {
    /// Protocol selection, independent of physical transport.
    #[serde(deserialize_with = "crate::selection::protocol")]
    pub protocol: String,
    /// Required only for the LAW Analyzer Manager stack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law: Option<Law>,
    /// Explicit outbound behavior; omission disables queries and work lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<crate::Orders>,
    /// Must be one.
    pub schema_version: u32,
    /// Definition identity.
    pub name: Identifier,
    /// Transport settings, without endpoint.
    pub transport: Transport,
    /// Framing options.
    pub framing: Framing,
    /// Dialog settings.
    pub dialog: Dialog,
    /// Syntax settings.
    pub syntax: Syntax,
    /// Field and code selection.
    pub mappings: Option<Mappings>,
    /// Code tables for vendor syntaxes, without field selectors.
    pub codes: Option<CodeTables>,
    /// Required POCT reviewer policy.
    pub poct: Option<Poct>,
    /// Required installed VIDAS settings.
    pub vidas: Option<Vidas>,
    /// Required Autolyser polling policy.
    pub autolyser: Option<Autolyser>,
    /// Named vendor departures.
    pub deviations: Vec<MachineDeparture>,
    /// Installed applicability, required when departures are declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure_scope: Option<DepartureScope>,
    /// The choices the lab makes in Setup (ADR 0032); none in the snapshot,
    /// which holds the definition with the chosen values applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<MachineChoice>,
    /// Scoped departures enabled only by a declared mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choice_departures: Vec<MachineDeparture>,
    /// Declared evidence.
    pub evidence: Evidence,
    /// Optional evidenced location of the analyzer software version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_version: Option<software_version::Declaration>,
}

/// References credentials by name; stores no secret material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    /// Maximum trusted order-cache age in seconds, shared by every host path.
    #[serde(default = "default_order_freshness")]
    pub freshness_seconds: u64,
    /// Deadline in seconds for one whole lab-app HTTP request (a result delivery
    /// or one order page): connect, TLS, upload, the app storing and answering,
    /// and the body read. Validated as 1 through 120 seconds.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
    /// Destination identity.
    pub name: Identifier,
    /// HTTPS base URL without user info, query or fragment.
    pub base_url: crate::HttpsUrl,
    /// Host credential reference.
    pub api_key_name: Identifier,
    /// Host signing credential reference.
    pub signing_key_name: Identifier,
}

impl Destination {
    /// Order freshness a `[destination]` table gets when it omits `freshness_seconds`.
    pub const DEFAULT_FRESHNESS_SECONDS: u64 = 30;
    /// Request deadline a `[destination]` table gets when it omits
    /// `request_timeout_seconds`. It covers TLS setup over a slow WAN plus a few
    /// seconds of app storage, while a hung order poll still fails and retries
    /// well inside the default freshness, so one stalled page does not make the
    /// order cache untrusted.
    pub const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 10;
    /// Accepted `request_timeout_seconds`, in whole seconds.
    pub const REQUEST_TIMEOUT_SECONDS: std::ops::RangeInclusive<u64> = 1..=120;
}

/// Installation-specific values kept apart from protocol rules.
#[derive(Debug, Clone, Serialize)]
#[serde(into = "SiteFile")]
pub struct Site {
    /// Explicit retention in days (1 through 3650), or `keep` (ADR 0013 item 1).
    pub retention: crate::Retention,
    /// Must be one.
    pub schema_version: u32,
    /// Where the machine link connects.
    pub endpoint: crate::Endpoint,
    /// The only devices a listening (`tcp_server`) machine accepts
    /// connections from; absent accepts any device that reaches the port,
    /// which the pages show as a warning (ADR 0026).
    pub allowed_peers: Option<crate::AllowedPeers>,
    /// Selected lab app; absent only for a record-only machine, which sends
    /// nothing to any lab app (ADR 0034).
    pub destination: Option<Destination>,
    /// The lab's time zone: an IANA name or a fixed `+HH:MM` offset. Only a LAW
    /// MSH-7 sent without an offset is read in it, with a note; absent keeps
    /// refusing such a header.
    pub time_zone: Option<crate::SiteTimeZone>,
    /// Present only for a record-only machine: the lab's answers to the
    /// capture profile's questions (ADR 0034).
    pub recording: Option<crate::ProfileAnswers>,
}

/// The site file as written: `endpoint` and `usb_serial` are two keys, and
/// loading turns them into one [`crate::Endpoint`]. Serializing writes the same
/// keys back, so canonical bytes of existing site files do not change.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SiteFile {
    retention: crate::Retention,
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usb_serial: Option<crate::UsbSerial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_peers: Option<crate::AllowedPeers>,
    /// The lab app written in the site file itself (the older form).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    destination: Option<Destination>,
    /// The name of a lab app in `lab-apps.toml`; never part of the effective
    /// snapshot, which holds the resolved settings under `destination`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lab_app: Option<Identifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time_zone: Option<crate::SiteTimeZone>,
    /// The lab's serial cable choice (ADR 0031); never part of the effective
    /// snapshot, which holds the resolved values under `transport.serial`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    serial: Option<SerialValues>,
    /// The lab's value for each choice the definition declares (ADR 0032);
    /// the snapshot holds the resolved values under its own `choices`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    choices: Option<ChoiceSelections>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    autolyser_poll_seconds: Option<u32>,
    /// Record only (ADR 0034): the machine is connected only to record its
    /// conversation; nothing goes to a lab app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recording: Option<crate::ProfileAnswers>,
}

/// What the lab chose in its site file beside the site settings: the serial
/// cable (ADR 0031) and the definition's choices (ADR 0032).
#[derive(Debug, Clone, Default)]
pub(crate) struct LabChoices {
    pub(crate) serial: Option<SerialValues>,
    pub(crate) choices: ChoiceSelections,
    pub(crate) autolyser_poll_seconds: Option<u32>,
}

impl SiteFile {
    /// Refuses a site file that sets both endpoint forms or neither, and resolves
    /// its lab app: its own `[destination]`, the named `lab_app`, or the only lab
    /// app in `lab_apps`. Several lab apps without a choice are refused. The
    /// lab's serial and definition choices come back beside the site.
    pub(crate) fn into_site(
        self,
        path: &str,
        lab_apps: &crate::LabApps,
    ) -> Result<(Site, crate::LabAppSource, LabChoices), crate::Error> {
        let (destination, source) = if self.recording.is_some() {
            // A record-only machine has no lab app; one written here would be
            // a destination nothing ever delivers to (ADR 0034).
            if self.destination.is_some() || self.lab_app.is_some() {
                return Err(crate::invalid(
                    path,
                    "lab_app",
                    "a record-only machine sends nothing to a lab app; remove lab_app and [destination], or remove [recording] to connect it normally",
                ));
            }
            (None, crate::LabAppSource::None)
        } else {
            let (destination, source) =
                lab_apps.resolve(path, self.destination, self.lab_app.as_ref())?;
            (Some(destination), source)
        };
        Ok((
            Site {
                retention: self.retention,
                schema_version: self.schema_version,
                endpoint: crate::Endpoint::choose(self.endpoint, self.usb_serial, path)?,
                allowed_peers: self.allowed_peers,
                destination,
                time_zone: self.time_zone,
                recording: self.recording,
            },
            source,
            LabChoices {
                serial: self.serial,
                choices: self.choices.unwrap_or_default(),
                autolyser_poll_seconds: self.autolyser_poll_seconds,
            },
        ))
    }
}

impl Site {
    /// The name of the lab app this machine sends to; none for a record-only
    /// machine (ADR 0034).
    #[must_use]
    pub fn lab_app_name(&self) -> Option<&Identifier> {
        self.destination
            .as_ref()
            .map(|destination| &destination.name)
    }
}

impl From<Site> for SiteFile {
    fn from(site: Site) -> Self {
        let (endpoint, usb_serial) = match site.endpoint {
            crate::Endpoint::Fixed(endpoint) => (Some(endpoint), None),
            crate::Endpoint::UsbSerial(serial) => (None, Some(serial)),
        };
        Self {
            retention: site.retention,
            schema_version: site.schema_version,
            endpoint,
            usb_serial,
            allowed_peers: site.allowed_peers,
            destination: site.destination,
            lab_app: None,
            time_zone: site.time_zone,
            serial: None,
            choices: None,
            autolyser_poll_seconds: None,
            recording: site.recording,
        }
    }
}

/// Explicit replacements; omitted sections retain their preceding layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Explicit replacement of LAW peer declarations and query policy.
    pub law: Option<Law>,
    /// Explicit replacement of outbound behavior.
    pub orders: Option<crate::Orders>,
    /// Must be one.
    pub schema_version: u32,
    /// Replacement transport settings.
    pub transport: Option<Transport>,
    /// Replacement envelope.
    pub framing: Option<Framing>,
    /// Replacement dialog.
    pub dialog: Option<Dialog>,
    /// Replacement syntax.
    pub syntax: Option<Syntax>,
    /// Replacement mapping table.
    pub mappings: Option<Mappings>,
    /// Replacement deviation declarations.
    pub deviations: Option<Vec<MachineDeparture>>,
    /// Explicit installed applicability replacement.
    pub departure_scope: Option<DepartureScope>,
    /// Replacement site endpoint; replaces a site `usb_serial` too.
    pub endpoint: Option<String>,
    /// Replacement USB adapter serial number; replaces a site `endpoint` too.
    pub usb_serial: Option<crate::UsbSerial>,
    /// Replacement destination.
    pub destination: Option<Destination>,
}

/// VIDAS result identity from the sample ID, assay and completion date and time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VidasIdentity {
    /// Assigning namespace for the VIDAS sample ID used as order context.
    pub sample_namespace: Identifier,
    /// Manual evidence for this installed mapping.
    pub evidence: IdentityReference,
}

/// The machine's code system and unit spellings for vendor protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeTables {
    /// Optional documented VIDAS completion recipe. Other vendor stacks refuse it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vidas_identity: Option<VidasIdentity>,
    /// The machine's own test code system (ADR 0029); vendor messages name none.
    pub test_system: Identifier,
    /// Original unit to explicit UCUM spelling.
    pub units: Vec<UnitMapping>,
}

/// Installed VIDAS sample ID ceiling; only documented layouts are representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum VidasSampleLimit {
    /// Twelve-character installed layout.
    Twelve,
    /// Seventeen-character installed layout.
    Seventeen,
}

impl TryFrom<u8> for VidasSampleLimit {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            12 => Ok(Self::Twelve),
            17 => Ok(Self::Seventeen),
            _ => Err("VIDAS sample_limit must be 12 or 17"),
        }
    }
}

impl From<VidasSampleLimit> for u8 {
    fn from(value: VidasSampleLimit) -> Self {
        match value {
            VidasSampleLimit::Twelve => 12,
            VidasSampleLimit::Seventeen => 17,
        }
    }
}

/// Exact source codes for dilution application; all other values stay unknown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DilutionSource {
    /// Group-relative source field.
    pub field: Field,
    /// Exact already-applied code.
    pub yes: Identifier,
    /// Exact not-applied code.
    pub no: Identifier,
}

/// Expected source value interpretation, without clinical interpretation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Require an exact numeric interpretation.
    Numeric,
    /// Require qualitative text or a coded interpretation.
    Coded,
    /// Preserve existing protocol interpretation.
    #[default]
    Either,
}

fn default_order_freshness() -> u64 {
    Destination::DEFAULT_FRESHNESS_SECONDS
}

fn default_request_timeout() -> u64 {
    Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS
}

/// Installed POCT identity evidence, never inferred from values or receipt IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoctIdentity {
    /// Require the device sequence in addition to device, observation time and analyte.
    pub sequence_number: bool,
    /// Explicit installed processing mode; no source mode is inferred from Hello.
    pub processing: analyte_model::ProcessingMode,
}
