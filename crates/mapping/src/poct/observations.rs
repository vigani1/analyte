//! Faithful POCT-to-common-model mapping, without clinical decisions or guessed IDs.
use analyte_model as model;
use analyte_poct::{Body, Code, Message, Nullable, Observation, Role, Service, Timestamp, Value};

/// A mapping failure requires retaining the receipt for operator handling.
#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    /// This message carries no observations.
    #[error("message is not an observations transfer")]
    NotObservations,
    /// Source structure cannot be associated with its service.
    #[error(transparent)]
    Structure(#[from] analyte_poct::MessageError),
    /// Common-model text limits were exceeded; original bytes remain protected.
    #[error(transparent)]
    Text(#[from] model::TextError),
    /// Common-model list capacity was exceeded.
    #[error(transparent)]
    List(#[from] model::ListError),
    /// Source offset cannot be represented by the common model.
    #[error(transparent)]
    Offset(#[from] model::OffsetError),
    /// The analyte is explicitly null and cannot become a guessed test code.
    #[error("observation analyte identity is null")]
    NullAnalyte,
}

/// One mapped result beside POCT evidence that is richer than the common summary.
/// The host retains the original Message and raw receipt for every interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedObservation {
    /// Shared result representation, preserving exact values, units and source flags.
    pub part: model::ResultPart,
    /// Original analyte coding, including display, system OID and version.
    pub source_code: Code,
    /// Original typed result, including explicit nulls and all coding attributes.
    pub source_value: Value,
    /// Source classification, including distinctions absent from the common summary.
    pub role: Role,
    /// Patient ID is patient context, never substituted for a specimen ID.
    pub patient_id: Option<model::SourceText>,
    /// Explicit specimen identity if reported. Missing remains missing.
    pub sample: Option<model::SampleIdentity>,
    /// POCT alone does not establish a globally stable run and revision identity.
    pub identity: model::IdentityStatus,
    /// Source observation index for examining all preserved optional metadata.
    pub source_index: usize,
}

/// Maps every observation, in source order, without I/O or automatic verification.
/// `received_at` comes from the host clock; it never replaces source time.
/// # Errors
/// Returns specific errors for non-observation messages, unrepresentable common
/// model evidence, null analyte identifiers and broken source associations.
pub fn map(
    message: &Message,
    received_at: jiff::Timestamp,
) -> Result<Vec<MappedObservation>, MappingError> {
    let services = match message.body() {
        Body::PatientObservations(s) | Body::NonPatientObservations(s) => s,
        Body::Hello { .. }
        | Body::Status { .. }
        | Body::EndTopic(_)
        | Body::Terminate(_)
        | Body::Acknowledgement { .. }
        | Body::Escape { .. }
        | Body::Request(_) => return Err(MappingError::NotObservations),
    };
    let mut mapped = Vec::new();
    for service in services {
        for observation in &service.observations {
            mapped.push(map_one(message, service, observation, received_at)?);
        }
    }
    Ok(mapped)
}

fn map_one(
    message: &Message,
    service: &Service,
    observation: &Observation,
    received_at: jiff::Timestamp,
) -> Result<MappedObservation, MappingError> {
    let document = message.document();
    let source_code = match &observation.code {
        Nullable::Value(c) => c.clone(),
        Nullable::Null(_) => return Err(MappingError::NullAnalyte),
    };
    let flags = observation
        .interpretation
        .iter()
        .map(|s| model::SourceText::new(s))
        .collect::<Result<Vec<_>, _>>()?;
    let units = match &observation.value {
        Value::Quantity(Nullable::Value(q)) => {
            Some(model::Units::original(model::SourceText::new(&q.unit)?))
        }
        Value::Quantity(Nullable::Null(_))
        | Value::Text(_)
        | Value::Code(_)
        | Value::Time(_)
        | Value::Integer(_)
        | Value::Real(_)
        | Value::Interval(_)
        | Value::Encapsulated(_) => None,
    };
    let comments = comments(document, service.source_index, observation.source_index)?;
    let source_status = optional_literal(document, observation.source_index, "OBS.status_cd");
    let mut times = Vec::new();
    if let Nullable::Value(time) = &service.observed {
        times.push(model::NamedSourceTime {
            role: model::Identifier::new("SVC.observation_dttm")?,
            time: map_time(time)?,
        });
    }
    let parent = document
        .elements()
        .get(observation.source_index)
        .and_then(analyte_poct::Element::parent)
        .ok_or(analyte_poct::MessageError::Structure)?;
    let patient_id = optional_literal(document, parent, "PT.patient_id")
        .map(|s| model::SourceText::new(&s))
        .transpose()?;
    let sample = if let Ok(specimen) = analyte_poct::child(document, service.source_index, "SPC") {
        optional_literal(document, specimen, "SPC.specimen_id")
            .map(|id| {
                model::SampleId::new(&id).map(|id| model::SampleIdentity {
                    id,
                    source: model::SampleSource::Unknown,
                    position: None,
                })
            })
            .transpose()?
    } else {
        None
    };
    let reference_range =
        optional_literal(document, observation.source_index, "OBS.normal_lo-hi_limit")
            .map(|v| model::SourceText::new(&v))
            .transpose()?;
    let part = model::ResultPart {
        test: code_id(&source_code)?,
        values: model::EvidenceList::new(vec![map_value(
            &observation.value,
            observation.interpretation.as_deref(),
        )?])?,
        units,
        dilution: Some(model::Dilution {
            original: None,
            factor: None,
            applied: model::DilutionApplied::Unknown,
        }),
        flags: model::EvidenceList::new(flags)?,
        reference_range,
        comments: model::EvidenceList::new(comments)?,
        qc_class: match service.role {
            Role::Patient => model::QcClass::Patient,
            Role::LiquidControl | Role::ElectronicControl => model::QcClass::Control,
            Role::Calibration | Role::CalibrationVerification => model::QcClass::Calibration,
            Role::Proficiency | Role::Unknown => model::QcClass::Unknown,
        },
        source_status: model::SourceText::new(source_status.as_deref().unwrap_or(""))?,
        // POCT acceptance/discard codes do not establish clinical finality.
        status: model::ResultStatus::Unknown,
        source_path: model::EvidenceList::new(vec![
            model::SourceNode {
                occurrence: 1,
                kind: model::Identifier::new(
                    document
                        .elements()
                        .first()
                        .ok_or(analyte_poct::MessageError::Structure)?
                        .name(),
                )?,
                sequence: None,
                status: None,
            },
            model::SourceNode {
                occurrence: source_occurrence(service.source_index)?,
                kind: model::Identifier::new("SVC")?,
                sequence: optional_literal(document, service.source_index, "SVC.sequence_nbr")
                    .map(|s| model::SourceText::new(&s))
                    .transpose()?,
                status: optional_literal(document, service.source_index, "SVC.status_cd")
                    .map(|s| model::SourceText::new(&s))
                    .transpose()?,
            },
            model::SourceNode {
                occurrence: source_occurrence(parent)?,
                kind: model::Identifier::new(
                    document
                        .elements()
                        .get(parent)
                        .ok_or(analyte_poct::MessageError::Structure)?
                        .name(),
                )?,
                sequence: None,
                status: None,
            },
            model::SourceNode {
                occurrence: source_occurrence(observation.source_index)?,
                kind: model::Identifier::new("OBS")?,
                sequence: None,
                status: None,
            },
        ])?,
        times: model::EvidenceList::new(times)?,
        received_at,
    };
    Ok(MappedObservation {
        part,
        source_code,
        source_value: observation.value.clone(),
        role: service.role,
        patient_id,
        sample,
        identity: model::IdentityStatus::Unresolved(model::Reason::new(
            "POCT service sequence may wrap; trusted run and revision evidence is required",
        )?),
        source_index: observation.source_index,
    })
}

fn optional_literal(
    document: &analyte_poct::Document,
    parent: usize,
    name: &'static str,
) -> Option<String> {
    analyte_poct::literal(document, parent, name).ok()
}

fn comments(
    document: &analyte_poct::Document,
    service: usize,
    observation: usize,
) -> Result<Vec<model::SourceText>, MappingError> {
    let mut comments = Vec::new();
    for parent in [service, observation] {
        for (note, _) in analyte_poct::children(document, parent).filter(|(_, e)| e.name() == "NTE")
        {
            comments.push(model::SourceText::new(&analyte_poct::literal(
                document, note, "NTE.text",
            )?)?);
        }
    }
    Ok(comments)
}

fn code_id(code: &Code) -> Result<model::NamespacedId, model::TextError> {
    Ok(model::NamespacedId {
        namespace: model::Identifier::new(
            code.system
                .as_deref()
                .or(code.system_name.as_deref())
                .unwrap_or("POCT01"),
        )?,
        value: model::Identifier::new(&code.value)?,
    })
}

fn map_value(
    value: &Value,
    interpretation: Option<&str>,
) -> Result<model::ResultValue, MappingError> {
    let comparator = match interpretation {
        Some("<") => Some(model::Comparator::Less),
        Some(">") => Some(model::Comparator::Greater),
        Some(_) | None => None,
    };
    let (original, parsed) = match value {
        Value::Quantity(Nullable::Value(q)) => {
            let parsed = match model::Decimal::new(&q.value) {
                Ok(number) => {
                    model::ValueInterpretation::Numeric(model::NumericValue { number, comparator })
                }
                Err(_) => unsupported("POCT01.PQ", "numeric value exceeds common model precision")?,
            };
            (q.value.clone(), parsed)
        }
        Value::Code(Nullable::Value(code)) => (
            code.value.clone(),
            model::ValueInterpretation::Coded(model::CodedValue {
                code: code_id(code)?,
                text: model::SourceText::new(code.display.as_deref().unwrap_or(""))?,
            }),
        ),
        Value::Text(Nullable::Value(text)) => (text.clone(), model::ValueInterpretation::Text),
        Value::Quantity(Nullable::Null(null))
        | Value::Code(Nullable::Null(null))
        | Value::Text(Nullable::Null(null))
        | Value::Time(Nullable::Null(null))
        | Value::Integer(Nullable::Null(null))
        | Value::Real(Nullable::Null(null))
        | Value::Interval(Nullable::Null(null)) => (
            null.code().to_owned(),
            unsupported(
                "POCT01.NULL",
                "explicit source null; not a numeric zero or an empty success",
            )?,
        ),
        Value::Time(Nullable::Value(_))
        | Value::Integer(Nullable::Value(_))
        | Value::Real(Nullable::Value(_))
        | Value::Interval(Nullable::Value(_))
        | Value::Encapsulated(_) => return Err(analyte_poct::MessageError::ObservationValue.into()),
    };
    Ok(model::ResultValue {
        decoded_text: None,
        original: model::SourceText::new(&original)?,
        parsed,
    })
}

fn unsupported(kind: &str, reason: &str) -> Result<model::ValueInterpretation, model::TextError> {
    Ok(model::ValueInterpretation::Unsupported {
        source_type: model::Identifier::new(kind)?,
        reason: model::Reason::new(reason)?,
    })
}

fn map_time(time: &Timestamp) -> Result<model::SourceTime, MappingError> {
    let original = time.original();
    let local = if time.offset_seconds().is_some() {
        original
            .get(
                ..original
                    .len()
                    .checked_sub(6)
                    .ok_or(analyte_poct::MessageError::Structure)?,
            )
            .ok_or(analyte_poct::MessageError::Structure)?
    } else {
        original
    };
    let precision = if local.contains('.') {
        model::TimePrecision::FractionalSecond
    } else {
        match local.len() {
            4 => model::TimePrecision::Year,
            7 => model::TimePrecision::Month,
            10 => model::TimePrecision::Day,
            13 => model::TimePrecision::Hour,
            16 => model::TimePrecision::Minute,
            19 => model::TimePrecision::Second,
            _ => model::TimePrecision::Unknown,
        }
    };
    Ok(model::SourceTime {
        original: model::SourceText::new(original)?,
        parsed: local.parse::<jiff::civil::DateTime>().ok(),
        offset: time
            .offset_seconds()
            .map(model::UtcOffset::try_from)
            .transpose()?,
        precision,
    })
}

fn source_occurrence(index: usize) -> Result<usize, MappingError> {
    index
        .checked_add(1)
        .ok_or_else(|| analyte_poct::MessageError::Structure.into())
}
