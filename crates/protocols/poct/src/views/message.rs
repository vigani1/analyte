//! Reads mandatory POCT Basic Profile messages with exact source evidence.
//! Kept whole: POCT message variants and their typed field accessors define one message vocabulary.
use crate::{
    Document, Limits,
    syntax::structure,
    views::data::{self, Nullable, Timestamp, Value},
};

/// Typed message decoding failures, without patient data in display text.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    /// XML could not be read within configured bounds.
    #[error(transparent)]
    Xml(#[from] crate::Error),
    /// A required object or field is missing or out of order.
    #[error("required message part missing: {0}")]
    Missing(&'static str),
    /// A required concrete field has no value.
    #[error("required concrete value missing: {0}")]
    MissingValue(&'static str),
    /// Objects have an illegal order, repetition or containment.
    #[error("invalid message structure")]
    Structure,
    /// The root is outside the supported message set.
    #[error("unsupported message type")]
    UnsupportedMessage,
    /// An element is outside the supported object vocabulary.
    #[error("unsupported message element")]
    UnsupportedElement,
    /// A field has invalid POCT data type content.
    #[error(transparent)]
    Data(#[from] data::DataError),
    /// Every observation needs exactly one result representation.
    #[error("observation requires exactly one quantitative or qualitative value")]
    ObservationValue,
    /// The header version is not POCT01.
    #[error("unsupported POCT version")]
    Version,
    /// Optional header message type conflicts with the root element.
    #[error("header message type does not match root")]
    MessageType,
    /// Patient and nonpatient source roles contradict their message type.
    #[error("service role conflicts with observation message type")]
    Role,
}

/// A validated conversation-local message identity, never an observation identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ControlId(String);

impl ControlId {
    /// Constructs an identity from nonempty, bounded XML-safe text.
    /// # Errors
    /// Rejects empty text, text over 256 bytes and XML-forbidden characters.
    pub fn new(value: &str) -> Result<Self, MessageError> {
        if value.is_empty()
            || value.len() > 256
            || value
                .chars()
                .any(|c| c.is_control() || !crate::syntax::xml_character(c))
        {
            return Err(MessageError::MissingValue("HDR.control_id"));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact identity spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Common mandatory header; protocol version has already been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Conversation-local control identity.
    pub control_id: ControlId,
    /// Source creation time, retaining its written precision and offset.
    pub created: Timestamp,
}

/// Positive custody acknowledgement versus an application processing error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckKind {
    /// The receiver assumes custody.
    Accept,
    /// Processing failed; the sender retains custody.
    Error,
}

/// Supported connection profile advertised by a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Basic synchronous acknowledgements.
    Synchronous,
    /// Basic asynchronous observation acknowledgements.
    Asynchronous,
    /// Continuous operation is available, but still needs explicit negotiation.
    Continuous,
}

/// Parsed static capabilities. Advertised extensions are preserved, not enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// Advertised connection profile.
    pub profile: Profile,
    /// Advertised optional topics in source order.
    pub topics: Vec<data::Code>,
    /// Advertised directives in source order.
    pub directives: Vec<data::Code>,
    /// Device receive size as a POCT integer or explicit null.
    pub maximum_message_bytes: Nullable<String>,
}

/// Source service classification; no clinical judgment is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Patient observation.
    Patient,
    /// Liquid quality control.
    LiquidControl,
    /// Electronic quality control.
    ElectronicControl,
    /// Calibration verification.
    CalibrationVerification,
    /// Calibration.
    Calibration,
    /// Proficiency test.
    Proficiency,
    /// Explicit unknown role.
    Unknown,
}

/// One typed observation, with its position in the source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// Preferred analyte code; equivalent codings remain in the typed document.
    pub code: Nullable<data::Code>,
    /// Quantitative or qualitative result, retaining source text and units.
    pub value: Value,
    /// Source method code, never inferred.
    pub method: String,
    /// Source abnormal/range indicator, including off-scale comparators.
    pub interpretation: Option<String>,
    /// Index of the OBS object, for accessing all optional source metadata.
    pub source_index: usize,
}

/// One service with explicit source classification and time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// Source classification. Unknown stays unknown.
    pub role: Role,
    /// Source observation time or explicit null.
    pub observed: Nullable<Timestamp>,
    /// Observations belonging to this service's patient or control object.
    pub observations: Vec<Observation>,
    /// Index of SVC in the preserved document.
    pub source_index: usize,
}

/// Mandatory Basic Profile message bodies (plus reviewer requests for round trips).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Startup device identity and advertised capabilities.
    Hello {
        /// Exact device identity.
        device_id: String,
        /// Device application timeout in written seconds, or explicit null.
        application_timeout: Option<Nullable<String>>,
        /// Static capabilities if supplied.
        capabilities: Option<Capabilities>,
    },
    /// Device's dynamic status.
    Status {
        /// Number of new observations; null never means zero.
        new_observations: Nullable<String>,
    },
    /// Patient observations.
    PatientObservations(Vec<Service>),
    /// Nonpatient observations, including explicit QC and calibration roles.
    NonPatientObservations(Vec<Service>),
    /// End of a topic, not itself acknowledged.
    EndTopic(Nullable<data::Code>),
    /// Termination request, which requires an acknowledgement.
    Terminate(Nullable<data::Code>),
    /// Application acknowledgement with an exact reference to a prior message.
    Acknowledgement {
        /// Custody accepted or processing error.
        kind: AckKind,
        /// Prior message identity.
        reference: ControlId,
    },
    /// Protocol escape ends the current topic.
    Escape {
        /// Prior message identity.
        reference: ControlId,
        /// Source reason code.
        reason: String,
    },
    /// Request for a topic; supported for validating reviewer output.
    Request(Nullable<data::Code>),
    /// Recognized optional Keep Alive, retained for an unsupported-message response.
    KeepAlive,
}

/// A structurally checked message and its exact bytes. Debug omits all content.
#[derive(PartialEq, Eq)]
pub struct Message {
    document: Document,
    header: Header,
    body: Body,
    values: Vec<Option<Value>>,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message").finish_non_exhaustive()
    }
}

impl Message {
    /// Reads and validates one mandatory Basic Profile message.
    /// # Errors
    /// Reports XML errors, missing required parts, structural violations,
    /// unsupported versions, contradictory roles and invalid POCT values.
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Self, MessageError> {
        let document = Document::parse(bytes, limits)?;
        let root = document
            .elements()
            .first()
            .ok_or(MessageError::Structure)?
            .name();
        if !matches!(
            root,
            "KPA.R01"
                | "HEL.R01"
                | "DST.R01"
                | "OBS.R01"
                | "OBS.R02"
                | "EOT.R01"
                | "END.R01"
                | "ACK.R01"
                | "ESC.R01"
                | "REQ.R01"
        ) {
            return Err(MessageError::UnsupportedMessage);
        }
        let values = structure::validate(&document)?;
        let hdr = structure::child(&document, 0, "HDR")?;
        if structure::literal(&document, hdr, "HDR.version_id")? != "POCT01" {
            return Err(MessageError::Version);
        }
        if let Ok(i) = structure::child(&document, hdr, "HDR.message_type") {
            let e = document.elements().get(i).ok_or(MessageError::Structure)?;
            if data::attr(e, "V") != Some(root) {
                return Err(MessageError::MessageType);
            }
        }
        let header = Header {
            control_id: ControlId::new(&structure::literal(&document, hdr, "HDR.control_id")?)?,
            created: Timestamp::parse(&structure::literal(&document, hdr, "HDR.creation_dttm")?)?,
        };
        let body = parse_body(&document, &values, root)?;
        Ok(Self {
            document,
            header,
            body,
            values,
        })
    }

    /// Borrows the checked common header.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Returns the typed message body.
    #[must_use]
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// Returns exact bytes and all source metadata.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Returns the typed field at an element index; object nodes have no value.
    #[must_use]
    pub fn value(&self, index: usize) -> Option<&Value> {
        self.values.get(index).and_then(Option::as_ref)
    }
}

fn field<'a>(
    document: &Document,
    values: &'a [Option<Value>],
    parent: usize,
    name: &'static str,
) -> Result<&'a Value, MessageError> {
    values
        .get(structure::child(document, parent, name)?)
        .and_then(Option::as_ref)
        .ok_or(MessageError::MissingValue(name))
}

fn integer(
    document: &Document,
    values: &[Option<Value>],
    parent: usize,
    name: &'static str,
) -> Result<Nullable<String>, MessageError> {
    match field(document, values, parent, name)? {
        Value::Integer(value) => Ok(value.clone()),
        Value::Text(_)
        | Value::Code(_)
        | Value::Quantity(_)
        | Value::Time(_)
        | Value::Real(_)
        | Value::Interval(_)
        | Value::Encapsulated(_) => Err(MessageError::Structure),
    }
}

fn coded(
    document: &Document,
    values: &[Option<Value>],
    parent: usize,
    name: &'static str,
) -> Result<Nullable<data::Code>, MessageError> {
    match field(document, values, parent, name)? {
        Value::Code(value) => Ok(value.clone()),
        Value::Text(_)
        | Value::Quantity(_)
        | Value::Time(_)
        | Value::Integer(_)
        | Value::Real(_)
        | Value::Interval(_)
        | Value::Encapsulated(_) => Err(MessageError::Structure),
    }
}

fn parse_body(
    document: &Document,
    values: &[Option<Value>],
    root: &str,
) -> Result<Body, MessageError> {
    Ok(match root {
        "HEL.R01" => {
            let dev = structure::child(document, 0, "DEV")?;
            let application_timeout = if let Ok(dcp) = structure::child(document, dev, "DCP") {
                match field(document, values, dcp, "DCP.application_timeout")? {
                    Value::Real(v) => Some(v.clone()),
                    Value::Text(_)
                    | Value::Code(_)
                    | Value::Quantity(_)
                    | Value::Time(_)
                    | Value::Integer(_)
                    | Value::Interval(_)
                    | Value::Encapsulated(_) => return Err(MessageError::Structure),
                }
            } else {
                None
            };
            let capabilities = structure::child(document, dev, "DSC")
                .ok()
                .map(|dsc| parse_capabilities(document, values, dsc))
                .transpose()?;
            Body::Hello {
                device_id: structure::literal(document, dev, "DEV.device_id")?,
                application_timeout,
                capabilities,
            }
        }
        "DST.R01" => Body::Status {
            new_observations: integer(
                document,
                values,
                structure::child(document, 0, "DST")?,
                "DST.new_observations_qty",
            )?,
        },
        "OBS.R01" => Body::PatientObservations(parse_services(document, values, true)?),
        "OBS.R02" => Body::NonPatientObservations(parse_services(document, values, false)?),
        "EOT.R01" => Body::EndTopic(coded(
            document,
            values,
            structure::child(document, 0, "EOT")?,
            "EOT.topic_cd",
        )?),
        "END.R01" => Body::Terminate(coded(
            document,
            values,
            structure::child(document, 0, "TRM")?,
            "TRM.reason_cd",
        )?),
        "ACK.R01" => {
            let ack = structure::child(document, 0, "ACK")?;
            let kind = match structure::literal(document, ack, "ACK.type_cd")?.as_str() {
                "AA" => AckKind::Accept,
                "AE" => AckKind::Error,
                _ => return Err(MessageError::Data(data::DataError::Code)),
            };
            Body::Acknowledgement {
                kind,
                reference: ControlId::new(&structure::literal(
                    document,
                    ack,
                    "ACK.ack_control_id",
                )?)?,
            }
        }
        "ESC.R01" => {
            let esc = structure::child(document, 0, "ESC")?;
            Body::Escape {
                reference: ControlId::new(&structure::literal(
                    document,
                    esc,
                    "ESC.esc_control_id",
                )?)?,
                reason: structure::literal(document, esc, "ESC.detail_cd")?,
            }
        }
        "KPA.R01" => Body::KeepAlive,
        "REQ.R01" => Body::Request(coded(
            document,
            values,
            structure::child(document, 0, "REQ")?,
            "REQ.request_cd",
        )?),
        _ => return Err(MessageError::UnsupportedMessage),
    })
}

fn parse_capabilities(
    document: &Document,
    values: &[Option<Value>],
    dsc: usize,
) -> Result<Capabilities, MessageError> {
    let profile = match structure::literal(document, dsc, "DSC.connection_profile_cd")?.as_str() {
        "SA" => Profile::Synchronous,
        "AA" => Profile::Asynchronous,
        "CS" => Profile::Continuous,
        _ => return Err(MessageError::Data(data::DataError::Code)),
    };
    let mut topics: Vec<data::Code> = Vec::new();
    let mut directives: Vec<data::Code> = Vec::new();
    for (i, e) in structure::children(document, dsc) {
        if matches!(
            e.name(),
            "DSC.topics_supported_cd" | "DSC.directives_supported_cd"
        ) {
            let Some(Some(Value::Code(Nullable::Value(code)))) = values.get(i) else {
                return Err(MessageError::Structure);
            };
            let set = if e.name() == "DSC.topics_supported_cd" {
                &mut topics
            } else {
                &mut directives
            };
            if set.iter().any(|member| {
                member.value == code.value
                    && member.system == code.system
                    && member.system_name == code.system_name
                    && member.version == code.version
            }) {
                return Err(MessageError::Data(data::DataError::Code));
            }
            set.push(code.clone());
        }
    }
    Ok(Capabilities {
        profile,
        topics,
        directives,
        maximum_message_bytes: integer(document, values, dsc, "DSC.max_message_sz")?,
    })
}

fn parse_services(
    document: &Document,
    values: &[Option<Value>],
    patient: bool,
) -> Result<Vec<Service>, MessageError> {
    let mut services = Vec::new();
    for (svc, _) in structure::children(document, 0).filter(|(_, e)| e.name() == "SVC") {
        let role = match structure::literal(document, svc, "SVC.role_cd")?.as_str() {
            "OBS" => Role::Patient,
            "LQC" => Role::LiquidControl,
            "EQC" => Role::ElectronicControl,
            "CVR" => Role::CalibrationVerification,
            "CAL" => Role::Calibration,
            "PRF" => Role::Proficiency,
            "UNK" => Role::Unknown,
            _ => return Err(MessageError::Role),
        };
        if (patient && !matches!(role, Role::Patient | Role::Unknown))
            || (!patient && role == Role::Patient)
        {
            return Err(MessageError::Role);
        }
        let observed = match field(document, values, svc, "SVC.observation_dttm")? {
            Value::Time(t) => t.clone(),
            Value::Text(_)
            | Value::Code(_)
            | Value::Quantity(_)
            | Value::Integer(_)
            | Value::Real(_)
            | Value::Interval(_)
            | Value::Encapsulated(_) => return Err(MessageError::Structure),
        };
        let subject = structure::child(document, svc, if patient { "PT" } else { "CTC" })?;
        let mut observations = Vec::new();
        for (obs, _) in structure::children(document, subject).filter(|(_, e)| e.name() == "OBS") {
            let value_name = if structure::child(document, obs, "OBS.value").is_ok() {
                "OBS.value"
            } else {
                "OBS.qualitative_value"
            };
            observations.push(Observation {
                code: coded(document, values, obs, "OBS.observation_id")?,
                value: field(document, values, obs, value_name)?.clone(),
                method: structure::literal(document, obs, "OBS.method_cd")?,
                interpretation: structure::literal(document, obs, "OBS.interpretation_cd").ok(),
                source_index: obs,
            });
        }
        services.push(Service {
            role,
            observed,
            observations,
            source_index: svc,
        });
    }
    Ok(services)
}

impl MessageError {
    /// Classifies a rejected message for the error ACK (Appendix B Table 14).
    #[must_use]
    pub fn acknowledgement_error(&self) -> crate::ApplicationError {
        match self {
            Self::Data(data::DataError::MissingValue) | Self::MissingValue(_) => {
                crate::ApplicationError::RequiredField
            }
            Self::Data(data::DataError::Code) => crate::ApplicationError::TableValue,
            Self::Xml(_)
            | Self::Data(
                data::DataError::Null
                | data::DataError::MissingUnit
                | data::DataError::Number
                | data::DataError::Time
                | data::DataError::Attribute
                | data::DataError::Interval,
            ) => crate::ApplicationError::DataType,
            Self::Missing(name) => {
                if name.contains('.') {
                    crate::ApplicationError::RequiredField
                } else {
                    crate::ApplicationError::ObjectSequence
                }
            }
            Self::Version => crate::ApplicationError::UnsupportedVersion,
            Self::UnsupportedMessage | Self::MessageType => {
                crate::ApplicationError::UnsupportedValue
            }
            Self::Structure | Self::UnsupportedElement | Self::ObservationValue | Self::Role => {
                crate::ApplicationError::ObjectSequence
            }
        }
    }
}

/// Recovers only an unambiguous Hello error-ACK reference from a complete header.
/// This never accepts a message or supplies missing data. Returns None if the
/// bounded prefix is invalid XML, is not Hello's first HDR, or has a missing,
/// null, repeated or invalid control ID. The caller retains the original failure.
#[must_use]
pub fn hello_error_reference(bytes: &[u8], limits: Limits) -> Option<ControlId> {
    if bytes.len() > limits.message_bytes {
        return None;
    }
    let mut reader = quick_xml::Reader::from_reader(bytes);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::End(end)) if end.name().as_ref() == "HDR" => {
                let length = usize::try_from(reader.buffer_position()).ok()?;
                let closing = b"</HEL.R01>";
                if length.checked_add(closing.len())? > limits.message_bytes {
                    return None;
                }
                let mut prefix = bytes.get(..length)?.to_vec();
                prefix.extend_from_slice(closing);
                // The normal bounded XML reader also rejects DTDs, entities and
                // invalid characters; scanning alone never establishes trust.
                let document = Document::parse(&prefix, limits).ok()?;
                if document.elements().first()?.name() != "HEL.R01" {
                    return None;
                }
                let mut children = structure::children(&document, 0);
                let (hdr, element) = children.next()?;
                if element.name() != "HDR" || children.next().is_some() {
                    return None;
                }
                let mut ids = structure::children(&document, hdr)
                    .filter(|(_, e)| e.name() == "HDR.control_id");
                let (index, id) = ids.next()?;
                if ids.next().is_some()
                    || structure::children(&document, index).next().is_some()
                    || id.attributes().iter().any(|(key, _)| key == "NULL")
                {
                    return None;
                }
                return ControlId::new(data::attr(id, "V")?).ok();
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => return None,
            // External XML events are scanned only to locate the header boundary;
            // the complete prefix must still pass Document::parse above.
            Ok(_) => {}
        }
    }
}
