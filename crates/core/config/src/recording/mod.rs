//! Builds record-only snapshots: the shipped capture profile of the model's protocol, the model's link and the lab's site file (ADR 0034).
//!
//! A record-only machine keeps the analyzer talking with the standard's own
//! link rules and keeps every byte; it reads no result, sends no order and
//! delivers nothing, so it needs no test, specimen or identity mapping and no
//! lab app. The profile is the same for every lab and ships in this crate; the
//! model contributes only its protocol and link (from its `machine.toml`, or
//! its `record.toml` while no definition is ready); the lab's site file gives
//! the connection, the cable and the answers in `[recording]`.
//!
//! A model whose analyzer offers a mode that changes its framing (the cobas
//! e 411's Elecsys or cobas protocol type) asks it in its `record.toml`
//! (`[analyzer_mode]`), and may declare the one ADR 0015 departure a recording
//! can need, packed ASTM records in one frame, scoped to the mode that packs.
//! The lab's answer selects it; no other mode, model or protocol gets it.
mod profile;
use profile::Profile;
pub use profile::{AstmCapture, AutolyserCapture, CaptureProtocol, Hl7Capture, VidasCapture};

use crate::stack::{DialogTimeouts, StreamTransport, SyntaxLimits};
use crate::{
    AckPoint, ChoiceLabel, ChoiceName, Error, Evidence, EvidenceLevel, LabAppSource, Machine,
    MachineDeparture, SerialValues, Site, Stack, Transport, TransportKind, Validated, invalid,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The file name of a model's link facts while it has no ready definition.
pub const RECORD_FILE: &str = "record.toml";

/// The receipt driver namespace of every record-only receipt; such a receipt
/// is a journaled chunk, kept and never read.
pub const RECORD_DRIVER: &str = "analyte-record";

/// Where an installed VIDAS PC expects the packet acknowledgment; the
/// installed mode cannot be guessed, so the lab answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VidasAcknowledgment {
    /// Literal: after the checksum, before ETX.
    Literal,
    /// Alternate: after ETX.
    Alternate,
}

/// The site file's `[recording]`: present only for a record-only machine,
/// with the lab's answers to the questions its protocol's profile leaves open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileAnswers {
    /// Autolyser only: seconds between next-report (R) requests, 1 to 3600.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_seconds: Option<u32>,
    /// VIDAS only: where the VIDAS PC expects the packet acknowledgment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledgment: Option<VidasAcknowledgment>,
    /// Autolyser only: whether its reports carry the controls block (the
    /// layout with controls enabled on the analyzer, Rev18 pp. 177 and 179).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<bool>,
    /// Only for a model whose record file asks it: the mode the analyzer's
    /// own settings screen shows, one of the values it offers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyzer_mode: Option<ChoiceName>,
}

/// A mode the analyzer offers on its own settings screen that changes how it
/// talks, asked in a model's `record.toml` (`[analyzer_mode]`). No value is
/// suggested: the installed mode cannot be guessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyzerMode {
    /// The question in plain words, naming where the analyzer shows it.
    pub label: ChoiceLabel,
    /// The values, 2 to 16, as the analyzer's screen names them.
    pub values: Vec<AnalyzerModeValue>,
}

/// One value of an [`AnalyzerMode`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyzerModeValue {
    /// The value's name, as the site file's `recording.analyzer_mode` writes it.
    pub value: ChoiceName,
    /// The value as the analyzer's screen or manual names it.
    pub label: ChoiceLabel,
}

impl AnalyzerMode {
    /// The allowed value names, joined for an error or a hint.
    #[must_use]
    pub fn allowed(&self) -> String {
        self.values
            .iter()
            .map(|value| value.value.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn names(&self) -> Vec<ChoiceName> {
        self.values
            .iter()
            .map(|value| value.value.clone())
            .collect()
    }

    /// Refuses fewer than two values, more than [`crate::MAX_CHOICE_VALUES`] or a repeated one.
    fn check(&self, path: &str) -> Result<(), Error> {
        let names = self.names();
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        if !(2..=crate::MAX_CHOICE_VALUES).contains(&names.len()) || unique.len() != names.len() {
            return Err(invalid(
                path,
                "analyzer_mode.values",
                "offer 2 to 16 different values",
            ));
        }
        Ok(())
    }
}

/// Seconds the lab may choose between Autolyser next-report requests.
pub const POLL_SECONDS: std::ops::RangeInclusive<u32> = 1..=3600;

/// A validated record-only stack: one protocol's capture settings and the link.
#[derive(Debug, Clone)]
pub struct RecordStack {
    link: RecordLink,
    transport: StreamTransport,
    limits: SyntaxLimits,
    timeouts: DialogTimeouts,
    /// The model's departures for the analyzer mode the lab chose.
    departures: Vec<MachineDeparture>,
}

/// The capture settings of the protocol a record-only connection speaks.
#[derive(Debug, Clone)]
pub enum RecordLink {
    /// ASTM LIS01-A2 link.
    Astm(AstmCapture),
    /// HL7 v2 over MLLP.
    Hl7(Hl7Capture),
    /// VIDAS Literal uploads.
    Vidas(VidasCapture),
    /// Autolyser next-report polling.
    Autolyser(AutolyserCapture),
}

impl RecordStack {
    /// The protocol's capture settings.
    #[must_use]
    pub const fn link(&self) -> &RecordLink {
        &self.link
    }
    /// The serial cable or TCP role in use.
    #[must_use]
    pub const fn transport(&self) -> &StreamTransport {
        &self.transport
    }
    /// The protocol this connection speaks.
    #[must_use]
    pub const fn protocol(&self) -> CaptureProtocol {
        match self.link {
            RecordLink::Astm(_) => CaptureProtocol::Astm,
            RecordLink::Hl7(_) => CaptureProtocol::Hl7,
            RecordLink::Vidas(_) => CaptureProtocol::Vidas,
            RecordLink::Autolyser(_) => CaptureProtocol::Autolyser,
        }
    }
    /// Whether a verified inbound ASTM frame may hold several records: the
    /// model declares packing (ADR 0015) for the analyzer mode the lab chose.
    #[must_use]
    pub fn packed_records(&self) -> bool {
        self.departures
            .iter()
            .any(|d| matches!(d, MachineDeparture::AstmPackedRecordsInFrame(_)))
    }
    pub(crate) const fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    pub(crate) const fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
}

/// A model's `record.toml`: its protocol and link, and the analyzer mode it
/// asks with the departures scoped to it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    recording_schema_version: u32,
    protocol: String,
    transport: ModelTransport,
    #[serde(default)]
    analyzer_mode: Option<AnalyzerMode>,
    #[serde(default)]
    deviations: Vec<MachineDeparture>,
}

/// What record only takes from a model file.
struct ModelFile {
    protocol: CaptureProtocol,
    transport: ModelTransport,
    analyzer_mode: Option<AnalyzerMode>,
    deviations: Vec<MachineDeparture>,
}

/// The link part of a model file: its kind and the manual's serial values.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelTransport {
    kind: TransportKind,
    #[serde(default)]
    serial: Option<SerialValues>,
}

/// A ready `machine.toml`'s link part; record only reads nothing else from it.
#[derive(Deserialize)]
struct DefinitionLink {
    protocol: String,
    transport: DefinitionTransport,
}

#[derive(Deserialize)]
struct DefinitionTransport {
    kind: TransportKind,
    #[serde(default)]
    serial: Option<SerialValues>,
}

/// Whether the text is a model's `record.toml` rather than a machine definition.
pub(crate) fn is_record_file(table: &toml::Table) -> bool {
    table.contains_key("recording_schema_version")
}

/// Whether a site file asks for a record-only connection (it has `[recording]`).
/// A file that does not parse is not record-only here; its loader reports why.
#[must_use]
pub fn records_only(site: &str) -> bool {
    site.len() <= crate::MAX_FILE_BYTES
        && toml::from_str::<toml::Table>(site).is_ok_and(|table| table.contains_key("recording"))
}

/// The protocol and link a model file names: a `record.toml` (with its
/// analyzer mode and departures), or the `protocol` and `[transport]` of a
/// ready definition.
fn model_link(path: &str, text: &str) -> Result<ModelFile, Error> {
    let table = crate::parse_table(path, text)?;
    if is_record_file(&table) {
        return record_link(path, table);
    }
    let link: DefinitionLink =
        toml::Value::Table(table)
            .try_into()
            .map_err(|source| Error::Schema {
                path: path.into(),
                source,
            })?;
    Ok(ModelFile {
        protocol: CaptureProtocol::of(&link.protocol, path)?,
        transport: ModelTransport {
            kind: link.transport.kind,
            serial: link.transport.serial,
        },
        analyzer_mode: None,
        deviations: Vec::new(),
    })
}

fn record_link(path: &str, table: toml::Table) -> Result<ModelFile, Error> {
    let file: RecordFile =
        toml::Value::Table(table)
            .try_into()
            .map_err(|source| Error::Schema {
                path: path.into(),
                source,
            })?;
    if file.recording_schema_version != 1 {
        return Err(invalid(
            path,
            "recording_schema_version",
            "use supported recording_schema_version = 1",
        ));
    }
    let protocol = CaptureProtocol::of(&file.protocol, path)?;
    if let Some(mode) = &file.analyzer_mode {
        mode.check(path)?;
    }
    let offered = file
        .analyzer_mode
        .as_ref()
        .map(AnalyzerMode::names)
        .unwrap_or_default();
    crate::validation::validate_recording(
        &file.deviations,
        protocol == CaptureProtocol::Astm,
        folder_key(path).as_deref(),
        &offered,
        path,
    )?;
    Ok(ModelFile {
        protocol,
        transport: file.transport,
        analyzer_mode: file.analyzer_mode,
        deviations: file.deviations,
    })
}

pub(crate) fn definition_details(
    path: &str,
    table: toml::Table,
) -> Result<crate::DefinitionDetails, Error> {
    let file = record_link(path, table)?;
    Ok(crate::DefinitionDetails {
        identity: None,
        protocol: file.protocol.key().into(),
        kind: file.transport.kind,
        serial: file.transport.serial,
        choices: Vec::new(),
        asks_autolyser_poll: false,
        asks_vidas_patient_namespace: false,
        analyzer_mode: file.analyzer_mode,
    })
}

/// The model's folder key, `vendor/model`, from the two folders its file is
/// in (ADR 0033); none for a file elsewhere, so no departure can match it.
fn folder_key(path: &str) -> Option<String> {
    let folder = std::path::Path::new(path).parent()?;
    let model = folder.file_name()?.to_str()?;
    let vendor = folder.parent()?.file_name()?.to_str()?;
    Some(format!("{vendor}/{model}"))
}

/// The analyzer mode the model asks, from its file; none when it asks none
/// or is a ready definition. Setup reads it to ask the question.
/// # Errors
/// Refuses what [`load`] refuses in the model file.
pub fn analyzer_mode(path: &str, text: &str) -> Result<Option<AnalyzerMode>, Error> {
    Ok(model_link(path, text)?.analyzer_mode)
}

/// The model's departures for the analyzer mode the site file answers; the
/// answer is required when the model asks it and refused when it does not.
fn chosen_departures(
    file: &ModelFile,
    answers: &ProfileAnswers,
    site: &str,
) -> Result<Vec<MachineDeparture>, Error> {
    let chosen = match (&file.analyzer_mode, &answers.analyzer_mode) {
        (None, None) => return Ok(Vec::new()),
        (None, Some(_)) => {
            return Err(invalid(
                site,
                "recording.analyzer_mode",
                "this model asks no analyzer mode; remove it",
            ));
        }
        (Some(mode), None) => {
            return Err(Error::MissingAnalyzerMode {
                path: site.into(),
                question: mode.label.as_str().to_owned(),
                allowed: mode.allowed(),
            });
        }
        (Some(mode), Some(chosen)) => {
            if !mode.values.iter().any(|value| &value.value == chosen) {
                return Err(Error::AnalyzerModeValue {
                    path: site.into(),
                    value: chosen.clone(),
                    allowed: mode.allowed(),
                });
            }
            chosen
        }
    };
    Ok(file
        .deviations
        .iter()
        .filter(|d| d.declaration().scope.mode.as_str() == chosen.as_str())
        .cloned()
        .collect())
}

/// Refuses a record-only site file on the normal loader: it loads only as a
/// recording, through [`crate::load_machine`].
pub(crate) fn refuse_recording_site(site: &Site, path: &str) -> Result<(), Error> {
    if site.recording.is_some() {
        return Err(invalid(
            path,
            "recording",
            "a record-only site file loads only as a recording (analyte_config::load_machine)",
        ));
    }
    Ok(())
}

/// Refuses a model's `record.toml` loaded for a normal connection, naming the way out.
pub(crate) fn refuse_record_file(path: &str, text: &str) -> Result<(), Error> {
    if crate::parse_table(path, text).is_ok_and(|table| is_record_file(&table)) {
        return Err(invalid(
            path,
            "recording_schema_version",
            "this model has no machine definition yet, so it can only record: choose Record only in Setup (or analyte machines set --record-only), which adds [recording] to the site file",
        ));
    }
    Ok(())
}

#[derive(Serialize)]
struct Canonical<'a> {
    schema_version: u32,
    recording: CanonicalRecording<'a>,
    site: &'a Site,
}

#[derive(Serialize)]
struct CanonicalRecording<'a> {
    protocol: CaptureProtocol,
    transport: &'a Transport,
    profile: &'a Profile,
    /// The departures in use for the chosen analyzer mode, exactly as declared.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    deviations: &'a [MachineDeparture],
}

/// Loads a record-only snapshot from a model file (`record.toml` or a ready
/// `machine.toml`) and a site file with `[recording]`; no overrides apply.
/// # Errors
/// Refuses a protocol without a capture profile, a link the protocol cannot
/// use, a lab app in the site file, a missing serial value, a departure the
/// record file cannot declare (ADR 0015 rules, narrowed for record only), and
/// a `[recording]` answer that is missing, out of range, not offered or for
/// another protocol.
pub fn load(model: (&str, &str), site: (&str, &str)) -> Result<Validated, Error> {
    let file = model_link(model.0, model.1)?;
    let (protocol, link) = (file.protocol, &file.transport);
    if !protocol.supports(link.kind) {
        return Err(invalid(
            model.0,
            "transport.kind",
            "this protocol cannot record over this link (HL7 runs over TCP; a watched folder has no conversation)",
        ));
    }
    if let Some(serial) = &link.serial {
        crate::validation::check_serial(serial, model.0, "transport.serial")?;
    }
    let (resolved, _, chosen) =
        crate::retention::load_site(site.0, site.1, &crate::LabApps::none(crate::LAB_APPS_FILE))?;
    let settings = resolved.recording.clone().ok_or_else(|| {
        invalid(
            site.0,
            "recording",
            "a record-only site file has a [recording] table",
        )
    })?;
    if chosen.vidas_patient_namespace.is_some() {
        return Err(invalid(
            site.0,
            "vidas_patient_namespace",
            "record-only connections do not send work lists",
        ));
    }
    if chosen.autolyser_poll_seconds.is_some() {
        return Err(invalid(
            site.0,
            "autolyser_poll_seconds",
            "record only takes its poll interval under [recording]",
        ));
    }
    if !chosen.choices.is_empty() {
        return Err(invalid(
            site.0,
            "choices",
            "record only asks its own questions in [recording]; remove [choices]",
        ));
    }
    let profile = protocol.profile()?;
    let mut transport = Transport {
        kind: link.kind,
        serial: link.serial.clone(),
        operation_timeout_ms: profile.operation_timeout_ms(),
    };
    if let Some(chosen) = &chosen.serial {
        crate::choose_serial(&mut transport, chosen, site.0)?;
    }
    crate::validation::validate_site(&resolved, &transport, site.0)?;
    let departures = chosen_departures(&file, &settings, site.0)?;
    let stack = build(&profile, &settings, &transport, site.0, departures)?;
    let document = toml::Value::try_from(Canonical {
        schema_version: 1,
        recording: CanonicalRecording {
            protocol,
            transport: &transport,
            profile: &profile,
            deviations: &stack.departures,
        },
        site: &resolved,
    })?;
    let canonical = toml::to_string(&document)?;
    let digest = Sha256::digest(canonical.as_bytes()).into();
    Ok(Validated {
        effective: crate::Effective {
            machine: Machine {
                software_version: None,
                name: analyte_model::Identifier::new(&format!("record-only-{}", protocol.key()))
                    .map_err(|_| invalid(model.0, "protocol", "unusable protocol name"))?,
                deviations: stack.departures.clone(),
                evidence: Evidence {
                    level: EvidenceLevel::Documented,
                    capabilities: Vec::new(),
                },
                stack: Stack::Record(Box::new(stack)),
            },
            site: resolved,
            source: LabAppSource::None,
            suggestion: link.serial.clone(),
            choices: Vec::new(),
            site_autolyser_poll_seconds: None,
            document,
        },
        canonical,
        digest,
    })
}

/// Builds the stack from the profile and the lab's answers; an answer for
/// another protocol is refused, so nothing in `[recording]` is silently unused.
fn build(
    profile: &Profile,
    settings: &ProfileAnswers,
    transport: &Transport,
    site: &str,
    departures: Vec<MachineDeparture>,
) -> Result<RecordStack, Error> {
    let stream = crate::stack::record_transport(transport, site)?;
    let unused = |key: &str| {
        invalid(
            site,
            &format!("recording.{key}"),
            "this protocol does not use this answer; remove it",
        )
    };
    if !matches!(profile, Profile::Autolyser(_)) && settings.poll_seconds.is_some() {
        return Err(unused("poll_seconds"));
    }
    if !matches!(profile, Profile::Autolyser(_)) && settings.controls.is_some() {
        return Err(unused("controls"));
    }
    if !matches!(profile, Profile::Vidas(_)) && settings.acknowledgment.is_some() {
        return Err(unused("acknowledgment"));
    }
    let (link, bytes, receive, send) = match profile {
        Profile::Astm(p) => (
            RecordLink::Astm(p.clone()),
            p.message_bytes(),
            p.receive_ms(),
            p.frame_ms(),
        ),
        Profile::Hl7(p) => (
            RecordLink::Hl7(p.clone()),
            p.message_bytes(),
            p.wait_ms(),
            p.wait_ms(),
        ),
        Profile::Vidas(p) => {
            let ack_point = match settings.acknowledgment {
                Some(VidasAcknowledgment::Literal) => AckPoint::Checksum,
                Some(VidasAcknowledgment::Alternate) => AckPoint::Etx,
                None => {
                    return Err(Error::MissingRecordingAnswer {
                        path: site.into(),
                        key: "acknowledgment",
                        question: "does the VIDAS PC use Literal or Alternate acknowledgment",
                    });
                }
            };
            let capture = VidasCapture::new(p.clone(), ack_point);
            let (bytes, receive, send) = (
                capture.frame_limit(),
                capture.session_ms(),
                capture.acknowledgment_ms(),
            );
            (RecordLink::Vidas(capture), bytes, receive, send)
        }
        Profile::Autolyser(p) => {
            let seconds = settings.poll_seconds.ok_or(Error::MissingRecordingAnswer {
                path: site.into(),
                key: "poll_seconds",
                question: "how many seconds between asks for the next report",
            })?;
            if !POLL_SECONDS.contains(&seconds) {
                return Err(invalid(
                    site,
                    "recording.poll_seconds",
                    "choose 1 to 3600 seconds between next-report requests",
                ));
            }
            let controls = settings.controls.ok_or(Error::MissingRecordingAnswer {
                path: site.into(),
                key: "controls",
                question: "do the analyzer's reports carry its controls (controls enabled in its setup)",
            })?;
            let capture = AutolyserCapture::new(p.clone(), seconds.saturating_mul(1000), controls);
            let (bytes, receive, send) = (
                capture.frame_limit(),
                capture.response_ms(),
                capture.acknowledgment_ms(),
            );
            (RecordLink::Autolyser(capture), bytes, receive, send)
        }
    };
    Ok(RecordStack {
        link,
        transport: stream,
        limits: SyntaxLimits::bytes_only(bytes),
        timeouts: DialogTimeouts {
            receive_timeout_ms: receive,
            send_timeout_ms: send,
        },
        departures,
    })
}
