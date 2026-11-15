//! Synthetic cases from plan §6, ADR 0002/0003 and AGENTS clinical rules 3–8.
use analyte_model::*;
use proptest::prelude::*;
use serde::{
    Deserialize,
    de::value::{Error, SeqDeserializer, StringDeserializer},
};

#[test]
fn adr0003_uuid_validation_survives_deserialization() {
    let invalid = StringDeserializer::<Error>::new("00000000-0000-0000-0000-000000000000".into());
    assert!(ReceiptId::deserialize(invalid).is_err());
    let valid = StringDeserializer::<Error>::new("0192a000-0000-7000-8000-000000000001".into());
    assert!(DeliveryId::deserialize(valid).is_ok());
}

#[test]
fn rule5_sample_identity_rejects_controls_empty_and_overlong_on_deserialize() {
    for raw in [String::new(), "A\nB".into(), "x".repeat(257)] {
        assert!(SampleId::deserialize(StringDeserializer::<Error>::new(raw)).is_err());
    }
    assert_ne!(SampleId::new("001a"), SampleId::new("001A"));
}

proptest! {
    #[test]
    fn rule5_sample_keeps_leading_zeros_and_case(raw in "[0-9A-Za-z]{1,256}") {
        let id = SampleId::new(&raw)?;
        prop_assert_eq!(id.as_str(), raw.as_str());
        let restored = SampleId::deserialize(StringDeserializer::<Error>::new(raw.clone()))?;
        prop_assert_eq!(restored, id);
    }
    #[test]
    fn rule5_decimal_keeps_precision(integer in "[0-9]{1,30}", fraction in "[0-9]{1,30}", sign in prop::sample::select(vec!["", "+", "-"])) {
        let raw = format!("{sign}{integer}.{fraction}");
        let decimal = Decimal::new(&raw)?;
        prop_assert_eq!(decimal.as_str(), raw.as_str());
        prop_assert_eq!(decimal.scale(), fraction.len());
    }
}

#[test]
fn rule5_no_guessed_decimal_or_lost_tokens() -> Result<(), Box<dyn std::error::Error>> {
    for raw in [
        "1,234", "<0.1", "NaN", "1e9", ".", "1.", "--1", "", "positive",
    ] {
        assert!(Decimal::new(raw).is_err());
        let value = ResultValue {
            original: SourceText::new(raw)?,
            parsed: ValueInterpretation::Text,
        };
        assert_eq!(value.original.as_str(), raw);
    }
    assert!(Decimal::deserialize(StringDeserializer::<Error>::new("1,234".into())).is_err());
    assert!(Decimal::new(&"1".repeat(257)).is_err());
    Ok(())
}

#[test]
fn rule6_units_require_matching_explicit_mapping() -> Result<(), Box<dyn std::error::Error>> {
    let original = SourceText::new("source-unit")?;
    let mapping = UnitMapping {
        source: original.clone(),
        target: Identifier::new("configured-ucum")?,
    };
    let units = Units::mapped(original.clone(), mapping.clone())?;
    assert_eq!(units.mapping(), Some(&mapping));
    assert_eq!(units.original_text(), &original);
    assert!(Units::mapped(SourceText::new("another-unit")?, mapping).is_err());
    assert!(Units::original(original).mapping().is_none());
    Ok(())
}

#[test]
fn rule7_lists_reject_unbounded_deserialization() {
    let seq = SeqDeserializer::<_, Error>::new(std::iter::repeat_n(1_u8, 257));
    assert!(EvidenceList::<u8>::deserialize(seq).is_err());
    assert!(EvidenceList::new(vec![1; 256]).is_ok());
    assert!(EvidenceList::new(vec![1; 257]).is_err());
}

#[test]
fn rule8_lifecycle_separates_acceptance_cancellation_and_uncertainty() -> Result<(), TransitionError>
{
    let sent = OrderState::Requested.transition(OrderState::Sent)?;
    let accepted = sent.transition(OrderState::Accepted)?;
    assert_eq!(accepted.transition(OrderState::Done)?, OrderState::Done);
    let cancel = accepted.transition(OrderState::CancelRequested)?;
    assert_eq!(
        cancel.transition(OrderState::CancelConfirmed)?,
        OrderState::CancelConfirmed
    );
    let uncertain = sent.transition(OrderState::Uncertain)?;
    assert_eq!(
        uncertain.transition(OrderState::Sent),
        Err(TransitionError {
            from: uncertain,
            to: OrderState::Sent
        })
    );
    assert!(uncertain.transition(OrderState::Requested).is_err());
    assert!(OrderState::Done.transition(OrderState::Sent).is_err());
    assert!(OrderState::Requested.transition(OrderState::Done).is_err());
    Ok(())
}

#[test]
fn plan6_only_production_delivers() {
    assert!(ProcessingMode::Production.may_deliver_to_production());
    for mode in [
        ProcessingMode::Unknown,
        ProcessingMode::Debug,
        ProcessingMode::Training,
        ProcessingMode::QualityControl,
    ] {
        assert!(!mode.may_deliver_to_production());
    }
}

#[test]
fn plan6_analyte_revision_is_independent_of_source_action() -> Result<(), Box<dyn std::error::Error>>
{
    let key = ObservationIdentity {
        code: NamespacedId {
            namespace: Identifier::new("synthetic-code-system")?,
            value: Identifier::new("TEST")?,
        },
        sub_id: SourceText::new("1")?,
        run: SourceText::new("")?,
        order: NamespacedId {
            namespace: Identifier::new("synthetic-app")?,
            value: Identifier::new("order-001")?,
        },
    };
    let initial = ResultIdentity {
        observation: key.clone(),
        revision: Revision(0),
    };
    let correction = ResultIdentity {
        observation: key,
        revision: initial.revision.next()?,
    };
    assert_ne!(initial, correction);
    assert_eq!(initial.observation, correction.observation);
    // A correction need not have a preceding result in memory or storage.
    assert!(matches!(
        IdentityStatus::Resolved(correction),
        IdentityStatus::Resolved(_)
    ));
    assert!(Revision(u64::MAX).next().is_err());
    let unresolved = IdentityStatus::Unresolved(Reason::new("missing order identity")?);
    assert!(matches!(unresolved, IdentityStatus::Unresolved(_)));
    Ok(())
}

#[test]
fn plan6_repeated_typed_and_unsupported_values_survive() -> Result<(), Box<dyn std::error::Error>> {
    let value = ResultValue {
        original: SourceText::new("raw composite")?,
        parsed: ValueInterpretation::Unsupported {
            source_type: Identifier::new("LOCAL")?,
            reason: Reason::new("unsupported value type")?,
        },
    };
    let repeated = EvidenceList::new(vec![value.clone(), value])?;
    assert_eq!(repeated.as_slice().len(), 2);
    assert_eq!(repeated.as_slice().first(), repeated.as_slice().last());
    Ok(())
}

#[test]
fn plan6_times_preserve_offset_and_written_precision() -> Result<(), Box<dyn std::error::Error>> {
    let time = SourceTime {
        original: SourceText::new("20261115120100.000+0100")?,
        parsed: Some("2026-11-15T12:01:00".parse()?),
        offset: Some(UtcOffset::try_from(3600)?),
        precision: TimePrecision::FractionalSecond,
    };
    assert_eq!(time.original.as_str(), "20261115120100.000+0100");
    assert_eq!(time.offset.map(i32::from), Some(3600));
    assert!(UtcOffset::try_from(i32::MAX).is_err());
    Ok(())
}

#[test]
fn plan6_replacement_units_cannot_be_empty_on_deserialize() {
    let seq = SeqDeserializer::<_, Error>::new(std::iter::empty::<u8>());
    assert!(NonEmptyEvidenceList::<u8>::deserialize(seq).is_err());
}

#[test]
fn adr0002_receipt_provenance_and_adr0003_conflicts_keep_both_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let first = ReceiptId::try_from(uuid::Uuid::parse_str(
        "0192a000-0000-7000-8000-000000000001",
    )?)?;
    let second = ReceiptId::try_from(uuid::Uuid::parse_str(
        "0192a000-0000-7000-8000-000000000002",
    )?)?;
    let named = NamespacedId {
        namespace: Identifier::new("synthetic")?,
        value: Identifier::new("v1")?,
    };
    let metadata = ReceiptMetadata {
        id: first,
        raw_digest: Digest([1; 32]),
        connection: Identifier::new("connection")?,
        machine: Identifier::new("machine")?,
        driver: named.clone(),
        config_version: Identifier::new("1")?,
        config_digest: Digest([2; 32]),
        received_at: jiff::Timestamp::UNIX_EPOCH,
    };
    let conflict = IdentityConflict {
        identity: ResultIdentity {
            observation: ObservationIdentity {
                code: named.clone(),
                sub_id: SourceText::new("1")?,
                run: SourceText::new("")?,
                order: named,
            },
            revision: Revision(0),
        },
        existing: metadata.id,
        incoming: second,
    };
    assert_ne!(conflict.existing, conflict.incoming);
    assert_ne!(metadata.raw_digest, metadata.config_digest);
    Ok(())
}

#[test]
fn plan6_multisegment_replacement_keeps_typed_repeats_and_parks_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let named = NamespacedId {
        namespace: Identifier::new("synthetic-system")?,
        value: Identifier::new("TEST")?,
    };
    let coded = ValueInterpretation::Coded(CodedValue {
        code: named.clone(),
        text: SourceText::new("display")?,
    });
    let structured = ValueInterpretation::StructuredNumeric(StructuredNumeric {
        comparator: Some(Comparator::Less),
        first: Some(Decimal::new("1.00")?),
        separator: SourceText::new(":")?,
        second: Some(Decimal::new("2")?),
    });
    let attachment = ValueInterpretation::Attachment(Attachment {
        media_type: Identifier::new("application/synthetic")?,
        encoding: Identifier::new("base64")?,
        data: SourceText::new("AA==")?,
    });
    let unsupported = ValueInterpretation::Unsupported {
        source_type: Identifier::new("LOCAL")?,
        reason: Reason::new("unsupported value type")?,
    };
    assert!(unsupported.is_unsupported());
    for value in [&coded, &structured, &attachment] {
        assert!(!value.is_unsupported());
    }
    let values = [coded, structured, attachment, unsupported]
        .into_iter()
        .map(|parsed| {
            Ok(ResultValue {
                original: SourceText::new("original composite")?,
                parsed,
            })
        })
        .collect::<Result<Vec<_>, TextError>>()?;
    let supported_part = ResultPart {
        test: named.clone(),
        values: EvidenceList::new(vec![ResultValue {
            original: SourceText::new("<0.10")?,
            parsed: ValueInterpretation::Numeric(NumericValue {
                number: Decimal::new("0.10")?,
                comparator: Some(Comparator::Less),
            }),
        }])?,
        units: None,
        dilution: None,
        flags: EvidenceList::new(vec![])?,
        reference_range: None,
        comments: EvidenceList::new(vec![])?,
        qc_class: QcClass::Patient,
        source_status: SourceText::new("F")?,
        status: ResultStatus::Final,
        source_path: EvidenceList::new(vec![])?,
        times: EvidenceList::new(vec![])?,
        received_at: jiff::Timestamp::UNIX_EPOCH,
    };
    let part = ResultPart {
        test: named.clone(),
        values: EvidenceList::new(values)?,
        units: None,
        dilution: Some(Dilution {
            factor: Some(Decimal::new("10")?),
            applied: DilutionApplied::Unknown,
        }),
        flags: EvidenceList::new(vec![])?,
        reference_range: None,
        comments: EvidenceList::new(vec![])?,
        qc_class: QcClass::Unknown,
        source_status: SourceText::new("C")?,
        status: ResultStatus::Corrected,
        source_path: EvidenceList::new(vec![])?,
        times: EvidenceList::new(vec![])?,
        received_at: jiff::Timestamp::UNIX_EPOCH,
    };
    let result = ResultContent {
        receipt: ReceiptId::try_from(uuid::Uuid::now_v7())?,
        sample: Some(SampleIdentity {
            id: SampleId::new("000123")?,
            source: SampleSource::ScannedByMachine,
            position: None,
        }),
        processing: ProcessingMode::Production,
        identity: IdentityStatus::Resolved(ResultIdentity {
            observation: ObservationIdentity {
                code: named.clone(),
                sub_id: SourceText::new("1")?,
                run: SourceText::new("")?,
                order: named,
            },
            revision: Revision(0),
        }),
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![part.clone(), part])?)?,
    };
    assert_eq!(result.parts.as_slice().len(), 2);
    assert!(result.requires_parking());
    assert!(!result.may_deliver_to_production());

    // Without its unsupported part, the same result could be delivered, but only
    // in production mode and only with a sample identity.
    let supported = ResultContent {
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![supported_part.clone()])?)?,
        ..result.clone()
    };
    assert!(supported.may_deliver_to_production());
    let training = ResultContent {
        processing: ProcessingMode::Training,
        ..supported.clone()
    };
    assert!(!training.may_deliver_to_production());
    let no_sample = ResultContent {
        sample: None,
        ..supported
    };
    assert!(no_sample.requires_parking());
    assert_eq!(
        result
            .parts
            .as_slice()
            .first()
            .map(|p| p.source_status.as_str()),
        Some("C")
    );
    Ok(())
}

#[test]
fn adr0003_run_evidence_distinguishes_reruns_without_using_source_status()
-> Result<(), Box<dyn std::error::Error>> {
    let code = NamespacedId {
        namespace: Identifier::new("synthetic")?,
        value: Identifier::new("TEST")?,
    };
    let no_run = ObservationIdentity {
        code: code.clone(),
        order: code,
        sub_id: SourceText::new("")?,
        run: SourceText::new("")?,
    };
    assert_eq!(no_run, no_run.clone());
    let first_run = ObservationIdentity {
        run: SourceText::new("1:1")?,
        ..no_run.clone()
    };
    let second_run = ObservationIdentity {
        run: SourceText::new("1:2")?,
        ..no_run
    };
    assert_ne!(first_run, second_run);
    Ok(())
}
