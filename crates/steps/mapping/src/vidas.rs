//! Maps VIDAS codes, original values and outgoing work.
use crate::inbound::evidence::{Error, map_part, map_result, map_test, map_units, push_result};
use crate::orders::codes::{self, map_priority};
use crate::orders::{AppOrder, map_patient};
use crate::{Output, Position};
use analyte_vidas::syntax::Subject;

use analyte_model::{
    Dilution, DilutionApplied, EvidenceList, Identifier, IdentityStatus, NamedSourceTime,
    NamespacedId, ObservationIdentity, QcClass, Reason, ResultDetails, ResultIdentity, ResultValue,
    SampleId, SampleIdentity, SampleSource, SourceText, SourceTime, TimePrecision, Units,
    ValueInterpretation,
};
/// Maps VIDAS values and resolves identity only with documented completion evidence.
/// # Errors
/// Preserves syntax, model and mapping failures.
pub fn map(
    machine: &analyte_config::VidasStack,
    message: &analyte_vidas::syntax::Message,
    input: crate::Input<'_>,
) -> Result<Output, Error> {
    let receipt = input.receipt;
    let time = input.received_at;
    let evidence = map_evidence(message)?;
    let mut output = Output::default();
    let Some(test) = map_test(machine.settings().codes(), evidence.assay.as_str(), None)? else {
        output.missing_test(
            Position {
                record: 1,
                field: 0,
            },
            EvidenceList::new(vec![evidence.value])?,
        )?;
        return Ok(output);
    };
    let mut source = map_part(test, evidence.value, time)?;
    let completion = completion_time(message)?;
    source.details = result_details(message, &evidence.subject)?;
    source.units = evidence.unit.map(Units::original);
    source.flags = EvidenceList::new(evidence.flag.into_iter().collect())?;
    source.comments = EvidenceList::new(vec![evidence.call])?;
    if let Some(original) = evidence.dilution {
        source.dilution = Some(Dilution {
            original: Some(original),
            factor: None,
            applied: DilutionApplied::Unknown,
        });
    }
    map_units(machine.settings().codes(), &mut source)?;
    let (sample, identity) = match evidence.subject {
        Subject::Sample(id) => {
            let identity = result_identity(
                machine,
                message,
                &completion.time,
                &id,
                &source.test,
                input.revision,
            )?;
            (
                Some(SampleIdentity {
                    id,
                    source: SampleSource::Unknown,
                    position: None,
                }),
                identity,
            )
        }
        Subject::KitControl { .. } | Subject::QualityControl(_) => {
            source.qc_class = QcClass::Control;
            (
                None,
                IdentityStatus::Unresolved(Reason::new(
                    "VIDAS control lacks trusted run identity",
                )?),
            )
        }
    };
    source.times = EvidenceList::new(vec![completion])?;
    push_result(&mut output, map_result(receipt, sample, source, identity)?)?;
    Ok(output)
}

fn result_details(
    message: &analyte_vidas::syntax::Message,
    subject: &Subject,
) -> Result<Option<ResultDetails>, Error> {
    let field = |tag| message.get(tag).map(SourceText::new).transpose();
    let mut details = ResultDetails {
        instrument_id: field("id")?,
        instrument_serial: field("sn")?,
        technologist: field("m4")?,
        ..ResultDetails::default()
    };
    match subject {
        Subject::Sample(_) => {}
        Subject::KitControl { name, lot } => {
            details.specimen_label = field("ci")?;
            details.control_name = Some(name.clone());
            details.strip_lot = Some(lot.clone());
        }
        Subject::QualityControl(name) => {
            details.control_name = Some(name.clone());
        }
    }
    Ok((details != ResultDetails::default()).then_some(details))
}

fn completion_time(message: &analyte_vidas::syntax::Message) -> Result<NamedSourceTime, Error> {
    let date = message.get("td").unwrap_or_default();
    let time = message.get("tt").unwrap_or_default();
    let original = format!("{date} {time}");
    let valid = date.len() == 10
        && time.len() == 5
        && jiff::civil::DateTime::strptime("%Y/%m/%d %H:%M", &original).is_ok();
    Ok(NamedSourceTime {
        role: Identifier::new("completion")?,
        time: SourceTime {
            original: SourceText::new(&original)?,
            // The civil date-time type includes a second, which Literal does not send.
            parsed: None,
            offset: None,
            precision: if valid {
                TimePrecision::Minute
            } else {
                TimePrecision::Unknown
            },
        },
    })
}

fn result_identity(
    machine: &analyte_config::VidasStack,
    message: &analyte_vidas::syntax::Message,
    completion: &SourceTime,
    sample: &SampleId,
    code: &NamespacedId,
    revision: analyte_model::Revision,
) -> Result<IdentityStatus, Error> {
    // Rev C Figure 4-3 uses the same short assay code for both doses.
    if code.value.as_str() == "CMVA" {
        return Ok(IdentityStatus::Unresolved(Reason::new(
            "VIDAS CMVA dose has no unique component or run identifier",
        )?));
    }
    let Some(recipe) = &machine.settings().codes().vidas_identity else {
        return Ok(IdentityStatus::Unresolved(Reason::new(
            &crate::Unidentified::VidasLiteral.to_string(),
        )?));
    };
    // BCI Link 161150-486 rev C, p. 4-6: date and time are completion evidence.
    if completion.precision != TimePrecision::Minute {
        return Ok(IdentityStatus::Unresolved(Reason::new(
            "missing or unrecognized VIDAS completion date or time",
        )?));
    }
    let date = message.get("td").unwrap_or_default();
    let time = message.get("tt").unwrap_or_default();
    Ok(IdentityStatus::Resolved(ResultIdentity {
        observation: ObservationIdentity {
            code: code.clone(),
            sub_id: SourceText::new("")?,
            run: crate::identity_run(&[date, time])?,
            order: NamespacedId {
                value: Identifier::new(sample.as_str())?,
                namespace: recipe.sample_namespace.clone(),
            },
        },
        revision,
    }))
}

/// Extracts VIDAS common-model evidence without parsing or changing written values.
/// # Errors
/// Rejects non-result messages or common-model text violations.
pub fn map_evidence(
    message: &analyte_vidas::syntax::Message,
) -> Result<ResultEvidence, analyte_vidas::syntax::Error> {
    if message.kind() != analyte_vidas::syntax::Kind::Result {
        return Err(analyte_vidas::syntax::Error::NotResult);
    }
    let call = message
        .get("ql")
        .ok_or_else(|| analyte_vidas::syntax::Error::Field("ql".into()))?;
    // Rev C Appendix B distinguishes invalid measurements from invalid recalibration.
    // Literal carries printable ASCII only (chapter 4).
    let parsed = match call.to_ascii_uppercase().as_str() {
        "INVALID" | "INVALIDE" | "UNGUELTIG" | "INVALIDO" | "NON VALIDO" | "NIEPOPRAWNY"
        | "GECERSIZ" => ValueInterpretation::Invalid,
        _ => ValueInterpretation::Text,
    };
    Ok(ResultEvidence {
        subject: message.subject().clone(),
        assay: SourceText::new(
            message
                .get("rt")
                .ok_or_else(|| analyte_vidas::syntax::Error::Field("rt".into()))?,
        )?,
        value: ResultValue::new(
            SourceText::new(
                message
                    .get("qn")
                    .ok_or_else(|| analyte_vidas::syntax::Error::Field("qn".into()))?,
            )?,
            parsed,
        ),
        call: SourceText::new(call)?,
        unit: message.get("y3").map(SourceText::new).transpose()?,
        flag: message.get("nc").map(SourceText::new).transpose()?,
        dilution: message.get("qd").map(SourceText::new).transpose()?,
    })
}

/// Source fields and explicit invalid-call evidence, before result identity is assigned.
#[derive(Debug, Clone)]
pub struct ResultEvidence {
    /// Exact source sample or control evidence.
    pub subject: Subject,
    /// The machine's assay code as sent; no assay name guessing.
    pub assay: SourceText,
    /// Original measured text, including comparator, error token or embedded unit.
    pub value: ResultValue,
    /// Original qualitative call in the installed language.
    pub call: SourceText,
    /// Separately transmitted unit, if any; no inferred conversion.
    pub unit: Option<SourceText>,
    /// Calibration/control/lot flag; never a clinical judgement.
    pub flag: Option<SourceText>,
    /// Original ratio or factor; application status is unknown.
    pub dilution: Option<SourceText>,
}
/// Maps one explicitly authorized VIDAS request per configured assay.
/// # Errors
/// Preserves missing context, code mapping and wire-size refusals.
pub fn map_order(
    order: &AppOrder,
    machine: &analyte_config::VidasStack,
    settings: &analyte_config::VidasOrders,
) -> Result<Vec<analyte_vidas::syntax::Message>, OrderError> {
    let installed = &machine.policy();
    if !installed.download {
        return Err(crate::OrderMappingError::Disabled.into());
    }
    let patient = map_patient(order, settings.patient_namespace())?;
    if patient.len() > 17 {
        return Err(crate::OrderMappingError::PatientIdTooLong.into());
    }
    if order.sample.as_str().len() > usize::from(u8::from(installed.sample_limit)) {
        return Err(OrderError::SampleIdTooLong);
    }
    // BCI A section 4 has no specimen-type destination; so is origin and si is empty.
    if order.specimen_type.is_some() {
        return Err(crate::OrderMappingError::SpecimenTypeUnsupported.into());
    }
    if order.tests.is_empty()
        || order.tests.len() > 100
        || order.tests.len() > machine.settings().limits().records()
        || order.tests.len() > machine.settings().limits().repetitions()
    {
        return Err(OrderError::TestCount);
    }
    map_priority(settings.tables().priorities(), order.priority)?;
    let limit = match installed.sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    };
    let birth_date = order
        .patient_birth_date
        .as_ref()
        .map(encode_birth_date)
        .transpose()?;
    let mut fields = vec![("mt", "mpr"), ("pi", patient)];
    if let Some(name) = &order.patient_name {
        fields.push(("pn", name.as_str()));
    }
    if let Some(date) = birth_date.as_deref() {
        fields.push(("pb", date));
    }
    if let Some(value) = &order.patient_sex_code {
        if !matches!(
            value.as_str().as_bytes(),
            [b'A'..=b'Z' | b'a'..=b'z' | b'-']
        ) {
            return Err(OrderError::SexCode);
        }
        fields.push(("ps", value.as_str()));
    }
    if let Some(value) = &order.sample_origin {
        if value.as_str().contains(' ') {
            return Err(OrderError::Origin);
        }
        fields.push(("so", value.as_str()));
    }
    fields.extend([("si", ""), ("ci", order.sample.as_str())]);
    let mut requests = Vec::new();
    let mut total_bytes = 0usize;
    for test in &order.tests {
        let code = codes::map_test(settings.tables().test_system(), &test.code)?;
        // Guide C p. 4-6: reject codes that the analyzer would truncate.
        if code.is_empty()
            || code.len() > 4
            || code.bytes().any(|b| !(32..=126).contains(&b) || b == b'|')
        {
            return Err(OrderError::AssayCode);
        }
        let required = installed
            .dilution_required_assays
            .contains(&test.code.value);
        let dilution = match (required, test.dilution.map(std::num::NonZeroU16::get)) {
            (true, Some(value @ 1..=999)) => Some(value.to_string()),
            (true, None) => return Err(OrderError::MissingDilution),
            (true, Some(_)) => return Err(OrderError::DilutionRange),
            (false, Some(_)) => return Err(OrderError::DilutionNotConfigured),
            (false, None) => None,
        };
        let mut request_fields = fields.clone();
        request_fields.push(("rt", code));
        if let Some(value) = dilution.as_deref() {
            request_fields.push(("qd", value));
        }
        let request = analyte_vidas::syntax::Message::new(&request_fields, limit)?;
        if request.text().len() > machine.settings().frame_limit() {
            return Err(OrderError::FrameLimit);
        }
        total_bytes = total_bytes
            .checked_add(request.text().len())
            .ok_or(OrderError::ByteCountOverflow)?;
        if total_bytes > machine.settings().limits().bytes() {
            return Err(OrderError::MessageLimit);
        }
        requests.push(request);
    }
    Ok(requests)
}

// Guide C p. 4-5 uses slash-delimited dates on the wire.
fn encode_birth_date(value: &SourceText) -> Result<String, OrderError> {
    let source = value.as_str();
    let date = source
        .parse::<jiff::civil::Date>()
        .map_err(|_| OrderError::BirthDate)?;
    if !source
        .as_bytes()
        .get(..4)
        .is_some_and(|year| year.iter().all(u8::is_ascii_digit))
        || date.to_string() != source
    {
        return Err(OrderError::BirthDate);
    }
    Ok(source.replace('-', "/"))
}

/// Specific outgoing VIDAS mapping refusals.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// Sample identity exceeds the vendor field width.
    #[error("overlong-sample-id")]
    SampleIdTooLong,
    /// Vendor syntax refuses the mapped request, without exposing patient content.
    #[error("VIDAS request is not representable: {0}")]
    Syntax(#[from] analyte_vidas::syntax::Error),
    /// VIDAS test count is empty or exceeds configured bounds.
    #[error("VIDAS test count is empty or exceeds configured bounds")]
    TestCount,
    /// A VIDAS assay code is 1 to 4 printable bytes without a pipe.
    #[error("VIDAS assay code must be 1 to 4 printable bytes without a pipe")]
    AssayCode,
    /// VIDAS request exceeds the frame limit.
    #[error("VIDAS request exceeds the frame limit")]
    FrameLimit,
    /// VIDAS request byte count overflow.
    #[error("VIDAS request byte count overflow")]
    ByteCountOverflow,
    /// VIDAS requests exceed the message byte limit.
    #[error("VIDAS requests exceed the message byte limit")]
    MessageLimit,
    /// The app birth date is not an ISO calendar date.
    #[error("invalid VIDAS birth date")]
    BirthDate,
    /// Patient sex must be a single installed code.
    #[error("invalid VIDAS patient sex code")]
    SexCode,
    /// Spaces in origin would be removed by the VIDAS PC.
    #[error("VIDAS sample origin contains spaces")]
    Origin,
    /// The instrument expects a dilution for the assay.
    #[error("required VIDAS dilution is missing")]
    MissingDilution,
    /// VIDAS PC dilution is limited to integers 1 through 999.
    #[error("VIDAS dilution must be in 1..=999")]
    DilutionRange,
    /// An unconfigured dilution could be silently ignored by the analyzer.
    #[error("VIDAS assay is not configured to receive dilution")]
    DilutionNotConfigured,
    /// Shared order evidence is unavailable or unmapped.
    #[error(transparent)]
    Shared(#[from] crate::OrderMappingError),
}
