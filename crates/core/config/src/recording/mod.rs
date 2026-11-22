//! Builds record-only snapshots: the shipped capture profile of the model's protocol, the model's link and the lab's site file (ADR 0034).
//!
//! A record-only machine keeps the analyzer talking with the standard's own
//! link rules and keeps every byte; it reads no result, sends no order and
//! delivers nothing, so it needs no test, specimen or identity mapping and no
//! lab app. The profile is the same for every lab and ships in this crate; the
//! model contributes only its protocol and link (from its `machine.toml`, or
//! its `record.toml` while no definition is ready); the lab's site file gives
//! the connection, the cable and the answers in `[recording]`.
mod profile;
use profile::Profile;
pub use profile::{AstmCapture, AutolyserCapture, CaptureProtocol, Hl7Capture, VidasCapture};

use crate::stack::{DialogTimeouts, StreamTransport, SyntaxLimits};
use crate::{
    AckPoint, Error, Evidence, EvidenceLevel, LabAppSource, Machine, SerialValues, Site, Stack,
    Transport, TransportKind, Validated, invalid,
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
    pub(crate) const fn limits(&self) -> &SyntaxLimits {
        &self.limits
    }
    pub(crate) const fn timeouts(&self) -> &DialogTimeouts {
        &self.timeouts
    }
}

/// A model's `record.toml`: its protocol and link, nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    recording_schema_version: u32,
    protocol: String,
    transport: ModelTransport,
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
fn is_record_file(table: &toml::Table) -> bool {
    table.contains_key("recording_schema_version")
}

/// Whether a site file asks for a record-only connection (it has `[recording]`).
/// A file that does not parse is not record-only here; its loader reports why.
#[must_use]
pub fn records_only(site: &str) -> bool {
    site.len() <= crate::MAX_FILE_BYTES
        && toml::from_str::<toml::Table>(site).is_ok_and(|table| table.contains_key("recording"))
}

fn parse_table(path: &str, text: &str) -> Result<toml::Table, Error> {
    if text.len() > crate::MAX_FILE_BYTES {
        return Err(Error::FileSize { path: path.into() });
    }
    toml::from_str(text).map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })
}

/// The protocol and link a model file names: a `record.toml`, or the
/// `protocol` and `[transport]` of a ready definition.
fn model_link(path: &str, text: &str) -> Result<(CaptureProtocol, ModelTransport), Error> {
    let table = parse_table(path, text)?;
    if is_record_file(&table) {
        let file: RecordFile = toml::from_str(text).map_err(|source| Error::Schema {
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
        return Ok((CaptureProtocol::of(&file.protocol, path)?, file.transport));
    }
    let link: DefinitionLink =
        toml::Value::Table(table)
            .try_into()
            .map_err(|source| Error::Schema {
                path: path.into(),
                source,
            })?;
    Ok((
        CaptureProtocol::of(&link.protocol, path)?,
        ModelTransport {
            kind: link.transport.kind,
            serial: link.transport.serial,
        },
    ))
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
    if parse_table(path, text).is_ok_and(|table| is_record_file(&table)) {
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
}

/// Loads a record-only snapshot from a model file (`record.toml` or a ready
/// `machine.toml`) and a site file with `[recording]`; no overrides apply.
/// # Errors
/// Refuses a protocol without a capture profile, a link the protocol cannot
/// use, a lab app in the site file, a missing serial value, and a
/// `[recording]` answer that is missing, out of range or for another protocol.
pub fn load(model: (&str, &str), site: (&str, &str)) -> Result<Validated, Error> {
    let (protocol, link) = model_link(model.0, model.1)?;
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
    let stack = build(&profile, &settings, &transport, site.0)?;
    let document = toml::Value::try_from(Canonical {
        schema_version: 1,
        recording: CanonicalRecording {
            protocol,
            transport: &transport,
            profile: &profile,
        },
        site: &resolved,
    })?;
    let canonical = toml::to_string(&document)?;
    let digest = Sha256::digest(canonical.as_bytes()).into();
    Ok(Validated {
        effective: crate::Effective {
            machine: Machine {
                name: analyte_model::Identifier::new(&format!("record-only-{}", protocol.key()))
                    .map_err(|_| invalid(model.0, "protocol", "unusable protocol name"))?,
                deviations: Vec::new(),
                evidence: Evidence {
                    level: EvidenceLevel::Documented,
                    capabilities: Vec::new(),
                },
                stack: Stack::Record(Box::new(stack)),
            },
            site: resolved,
            source: LabAppSource::None,
            suggestion: link.serial,
            choices: Vec::new(),
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
    })
}
