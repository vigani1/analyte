//! Converts model result, sample, identity and lifecycle values to contract types.
use super::{
    Attachment, CarrierKind, CodedValue, Comparator, Dilution, DilutionApplied, EvidenceList,
    IdentityStatus, NamedSourceTime, NamespacedId, NonEmptyEvidenceList, NumericValue,
    ObservationIdentity, OrderState, ProcessingMode, QcClass, ResultAttribute, ResultDocument,
    ResultIdentity, ResultPart, ResultStatus, ResultValue, SampleIdentity, SamplePosition,
    SampleSource, SourceNode, SourceTime, StructuredNumeric, TimePrecision, UnitMapping,
    ValueInterpretation,
};

impl From<&analyte_model::NumericValue> for NumericValue {
    fn from(v: &analyte_model::NumericValue) -> Self {
        Self {
            number: (&v.number).into(),
            comparator: v.comparator.as_ref().map(Into::into),
        }
    }
}

impl From<&analyte_model::ResultValue> for ResultValue {
    fn from(v: &analyte_model::ResultValue) -> Self {
        match v.parsed() {
            analyte_model::ValueInterpretation::Text => {
                Self::text((&v.original).into(), v.decoded_text().map(Into::into))
            }
            parsed @ (analyte_model::ValueInterpretation::Numeric(_)
            | analyte_model::ValueInterpretation::Invalid
            | analyte_model::ValueInterpretation::Coded(_)
            | analyte_model::ValueInterpretation::StructuredNumeric(_)
            | analyte_model::ValueInterpretation::Attachment(_)
            | analyte_model::ValueInterpretation::Unsupported { .. }) => {
                Self::new((&v.original).into(), parsed.into())
            }
        }
    }
}

impl From<&analyte_model::UnitMapping> for UnitMapping {
    fn from(v: &analyte_model::UnitMapping) -> Self {
        Self {
            source: (&v.source).into(),
            target: (&v.target).into(),
        }
    }
}

impl From<&analyte_model::Dilution> for Dilution {
    fn from(v: &analyte_model::Dilution) -> Self {
        Self {
            original: v.original.as_ref().map(Into::into),
            factor: v.factor.as_ref().map(Into::into),
            applied: (&v.applied).into(),
        }
    }
}

impl From<&analyte_model::CodedValue> for CodedValue {
    fn from(v: &analyte_model::CodedValue) -> Self {
        Self {
            code: (&v.code).into(),
            text: (&v.text).into(),
        }
    }
}

impl From<&analyte_model::StructuredNumeric> for StructuredNumeric {
    fn from(v: &analyte_model::StructuredNumeric) -> Self {
        Self {
            comparator: v.comparator.as_ref().map(Into::into),
            first: v.first.as_ref().map(Into::into),
            separator: (&v.separator).into(),
            second: v.second.as_ref().map(Into::into),
        }
    }
}

impl From<&analyte_model::Attachment> for Attachment {
    fn from(v: &analyte_model::Attachment) -> Self {
        Self {
            media_type: (&v.media_type).into(),
            encoding: (&v.encoding).into(),
            data: (&v.data).into(),
        }
    }
}

impl From<&analyte_model::SamplePosition> for SamplePosition {
    fn from(v: &analyte_model::SamplePosition) -> Self {
        Self {
            kind: (&v.kind).into(),
            carrier: v.carrier.as_ref().map(Into::into),
            slot: (&v.slot).into(),
        }
    }
}

impl From<&analyte_model::SampleIdentity> for SampleIdentity {
    fn from(v: &analyte_model::SampleIdentity) -> Self {
        Self {
            id: (&v.id).into(),
            source: (&v.source).into(),
            position: v.position.as_ref().map(Into::into),
        }
    }
}

impl From<&analyte_model::SourceTime> for SourceTime {
    fn from(v: &analyte_model::SourceTime) -> Self {
        Self {
            original: (&v.original).into(),
            parsed: v.parsed.as_ref().copied(),
            offset: v.offset.as_ref().map(|v| (*v).into()),
            precision: (&v.precision).into(),
        }
    }
}

impl From<&analyte_model::SourceNode> for SourceNode {
    fn from(v: &analyte_model::SourceNode) -> Self {
        Self {
            // usize always fits in u64 on supported targets; saturate rather
            // than wrap if a future platform ever disagrees.
            occurrence: u64::try_from(v.occurrence).unwrap_or(u64::MAX),
            kind: (&v.kind).into(),
            sequence: v.sequence.as_ref().map(Into::into),
            status: v.status.as_ref().map(Into::into),
        }
    }
}

impl From<&analyte_model::ResultAttribute> for ResultAttribute {
    fn from(v: &analyte_model::ResultAttribute) -> Self {
        use analyte_model::ResultAttribute as Model;
        match v {
            Model::ControlLabel(text) => Self::ControlLabel(text.into()),
            Model::ControlName(text) => Self::ControlName(text.into()),
            Model::ControlLot(text) => Self::ControlLot(text.into()),
            Model::ControlKnowledge(text) => Self::ControlKnowledge(text.into()),
            Model::ControlLevel(text) => Self::ControlLevel(text.into()),
            Model::ReportList(text) => Self::ReportList(text.into()),
            Model::SampleMedium(text) => Self::SampleMedium(text.into()),
            Model::InstrumentId(text) => Self::InstrumentId(text.into()),
            Model::InstrumentSerial(text) => Self::InstrumentSerial(text.into()),
            Model::Technologist(text) => Self::Technologist(text.into()),
        }
    }
}

impl From<&analyte_model::ResultPart> for ResultPart {
    fn from(v: &analyte_model::ResultPart) -> Self {
        Self {
            test: (&v.test).into(),
            values: EvidenceList(v.values.as_slice().iter().map(Into::into).collect()),
            attributes: v
                .attributes
                .as_ref()
                .map(|values| EvidenceList(values.as_slice().iter().map(Into::into).collect())),
            units: v.units.as_ref().map(Into::into),
            dilution: v.dilution.as_ref().map(Into::into),
            flags: EvidenceList(v.flags.as_slice().iter().map(Into::into).collect()),
            reference_range: v.reference_range.as_ref().map(Into::into),
            comments: EvidenceList(v.comments.as_slice().iter().map(Into::into).collect()),
            qc_class: (&v.qc_class).into(),
            source_status: (&v.source_status).into(),
            status: (&v.status).into(),
            source_path: EvidenceList(v.source_path.as_slice().iter().map(Into::into).collect()),
            times: EvidenceList(v.times.as_slice().iter().map(Into::into).collect()),
            received_at: v.received_at,
        }
    }
}

impl From<&analyte_model::NamedSourceTime> for NamedSourceTime {
    fn from(v: &analyte_model::NamedSourceTime) -> Self {
        Self {
            role: (&v.role).into(),
            time: (&v.time).into(),
        }
    }
}

impl From<&analyte_model::ResultContent> for ResultDocument {
    fn from(v: &analyte_model::ResultContent) -> Self {
        Self {
            receipt: v.receipt.into(),
            sample: v.sample.as_ref().map(Into::into),
            processing: (&v.processing).into(),
            identity: (&v.identity).into(),
            parts: NonEmptyEvidenceList(EvidenceList(
                v.parts.as_slice().iter().map(Into::into).collect(),
            )),
        }
    }
}

impl From<&analyte_model::NamespacedId> for NamespacedId {
    fn from(v: &analyte_model::NamespacedId) -> Self {
        Self {
            namespace: (&v.namespace).into(),
            value: (&v.value).into(),
        }
    }
}

impl From<&analyte_model::ResultIdentity> for ResultIdentity {
    fn from(v: &analyte_model::ResultIdentity) -> Self {
        Self {
            observation: (&v.observation).into(),
            revision: v.revision.into(),
        }
    }
}

impl From<&analyte_model::ObservationIdentity> for ObservationIdentity {
    fn from(v: &analyte_model::ObservationIdentity) -> Self {
        Self {
            code: (&v.code).into(),
            sub_id: (&v.sub_id).into(),
            run: (&v.run).into(),
            order: (&v.order).into(),
        }
    }
}

impl From<&analyte_model::SampleSource> for SampleSource {
    fn from(v: &analyte_model::SampleSource) -> Self {
        match v {
            analyte_model::SampleSource::ScannedByMachine => Self::ScannedByMachine,
            analyte_model::SampleSource::TypedOnMachine => Self::TypedOnMachine,
            analyte_model::SampleSource::WorkList => Self::WorkList,
            analyte_model::SampleSource::InstrumentAssigned => Self::InstrumentAssigned,
            analyte_model::SampleSource::Unknown => Self::Unknown,
        }
    }
}

impl From<&analyte_model::CarrierKind> for CarrierKind {
    fn from(v: &analyte_model::CarrierKind) -> Self {
        match v {
            analyte_model::CarrierKind::Rack => Self::Rack,
            analyte_model::CarrierKind::Disk => Self::Disk,
            analyte_model::CarrierKind::Tray => Self::Tray,
            analyte_model::CarrierKind::Other(v) => Self::Other(v.into()),
        }
    }
}

impl From<&analyte_model::QcClass> for QcClass {
    fn from(v: &analyte_model::QcClass) -> Self {
        match v {
            analyte_model::QcClass::Patient => Self::Patient,
            analyte_model::QcClass::Control => Self::Control,
            analyte_model::QcClass::Calibration => Self::Calibration,
            analyte_model::QcClass::Unknown => Self::Unknown,
        }
    }
}

impl From<&analyte_model::ResultStatus> for ResultStatus {
    fn from(v: &analyte_model::ResultStatus) -> Self {
        match v {
            analyte_model::ResultStatus::Pending => Self::Pending,
            analyte_model::ResultStatus::Preliminary => Self::Preliminary,
            analyte_model::ResultStatus::Final => Self::Final,
            analyte_model::ResultStatus::Corrected => Self::Corrected,
            analyte_model::ResultStatus::Withdrawn => Self::Withdrawn,
            analyte_model::ResultStatus::NotPerformed => Self::NotPerformed,
            analyte_model::ResultStatus::Unknown => Self::Unknown,
        }
    }
}

impl From<&analyte_model::TimePrecision> for TimePrecision {
    fn from(v: &analyte_model::TimePrecision) -> Self {
        match v {
            analyte_model::TimePrecision::Year => Self::Year,
            analyte_model::TimePrecision::Month => Self::Month,
            analyte_model::TimePrecision::Day => Self::Day,
            analyte_model::TimePrecision::Hour => Self::Hour,
            analyte_model::TimePrecision::Minute => Self::Minute,
            analyte_model::TimePrecision::Second => Self::Second,
            analyte_model::TimePrecision::FractionalSecond => Self::FractionalSecond,
            analyte_model::TimePrecision::Unknown => Self::Unknown,
        }
    }
}

impl From<&analyte_model::ProcessingMode> for ProcessingMode {
    fn from(v: &analyte_model::ProcessingMode) -> Self {
        match v {
            analyte_model::ProcessingMode::Unknown => Self::Unknown,
            analyte_model::ProcessingMode::Production => Self::Production,
            analyte_model::ProcessingMode::Training => Self::Training,
            analyte_model::ProcessingMode::Debug => Self::Debug,
            analyte_model::ProcessingMode::QualityControl => Self::QualityControl,
        }
    }
}

impl TryFrom<&analyte_model::OrderState> for OrderState {
    type Error = crate::DocumentError;
    fn try_from(v: &analyte_model::OrderState) -> Result<Self, Self::Error> {
        Ok(match v {
            analyte_model::OrderState::Sending => {
                return Err(crate::DocumentError::InternalOrderState);
            }
            analyte_model::OrderState::Requested => Self::Requested,
            analyte_model::OrderState::Sent => Self::Sent,
            analyte_model::OrderState::Accepted => Self::Accepted,
            analyte_model::OrderState::Done => Self::Done,
            analyte_model::OrderState::Rejected => Self::Rejected,
            analyte_model::OrderState::CancelRequested => Self::CancelRequested,
            analyte_model::OrderState::CancelConfirmed => Self::CancelConfirmed,
            analyte_model::OrderState::Uncertain => Self::Uncertain,
        })
    }
}

impl From<&analyte_model::Comparator> for Comparator {
    fn from(v: &analyte_model::Comparator) -> Self {
        match v {
            analyte_model::Comparator::Less => Self::Less,
            analyte_model::Comparator::LessOrEqual => Self::LessOrEqual,
            analyte_model::Comparator::Greater => Self::Greater,
            analyte_model::Comparator::GreaterOrEqual => Self::GreaterOrEqual,
        }
    }
}

impl From<&analyte_model::DilutionApplied> for DilutionApplied {
    fn from(v: &analyte_model::DilutionApplied) -> Self {
        match v {
            analyte_model::DilutionApplied::Yes => Self::Yes,
            analyte_model::DilutionApplied::No => Self::No,
            analyte_model::DilutionApplied::Unknown => Self::Unknown,
        }
    }
}

impl From<&analyte_model::IdentityStatus> for IdentityStatus {
    fn from(v: &analyte_model::IdentityStatus) -> Self {
        match v {
            analyte_model::IdentityStatus::Resolved(v) => Self::Resolved(v.into()),
            analyte_model::IdentityStatus::Unresolved(v) => Self::Unresolved(v.into()),
        }
    }
}

impl From<&analyte_model::ValueInterpretation> for ValueInterpretation {
    fn from(v: &analyte_model::ValueInterpretation) -> Self {
        match v {
            analyte_model::ValueInterpretation::Numeric(v) => Self::Numeric(v.into()),
            analyte_model::ValueInterpretation::Text => Self::Text,
            analyte_model::ValueInterpretation::Invalid => Self::Invalid,
            analyte_model::ValueInterpretation::Coded(v) => Self::Coded(v.into()),
            analyte_model::ValueInterpretation::StructuredNumeric(v) => {
                Self::StructuredNumeric(v.into())
            }
            analyte_model::ValueInterpretation::Attachment(v) => Self::Attachment(v.into()),
            analyte_model::ValueInterpretation::Unsupported {
                source_type,
                reason,
            } => Self::Unsupported {
                source_type: source_type.into(),
                reason: reason.into(),
            },
        }
    }
}

impl From<OrderState> for analyte_model::OrderState {
    fn from(state: OrderState) -> Self {
        match state {
            OrderState::Requested => Self::Requested,
            OrderState::Sent => Self::Sent,
            OrderState::Accepted => Self::Accepted,
            OrderState::Done => Self::Done,
            OrderState::Rejected => Self::Rejected,
            OrderState::CancelRequested => Self::CancelRequested,
            OrderState::CancelConfirmed => Self::CancelConfirmed,
            OrderState::Uncertain => Self::Uncertain,
        }
    }
}
