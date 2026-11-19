//! Tests parsed record storage and receipt associations.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

// Safety rule 3 (ADR 0003)
#[test]
fn interpretation_digest_checked_legacy_evidence_reads() -> TestResult {
    let evidence = interpretation()?;
    let mut encoded = serde_json::to_value(&evidence)?;
    encoded["config_digest"] = serde_json::to_value(digest(b"different"))?;
    assert!(
        serde_json::from_value::<Interpretation>(encoded.clone())
            .map(|_| ())
            .expect_err("invalid evidence must be refused")
            .is_data()
    );
    encoded
        .as_object_mut()
        .ok_or("not object")?
        .remove("config_digest");
    let legacy: Interpretation = serde_json::from_value(encoded)?;
    assert_eq!(legacy.config, evidence.config);
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn interpretation_roundtrip_keeps_provenance_and_failure_reason() -> TestResult {
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Failed(Reason::new("unknown synthetic test")?);
    let encoded = serde_json::to_value(&evidence)?;
    assert_eq!(
        encoded["config"], "config",
        "config is stored as its own text"
    );
    assert_eq!(
        encoded["config_digest"],
        serde_json::to_value(digest(b"config"))?
    );
    let decoded: Interpretation = serde_json::from_value(encoded.clone())?;
    assert_eq!(decoded.driver, evidence.driver);
    assert_eq!(decoded.config_version, evidence.config_version);
    assert_eq!(decoded.config, evidence.config);
    let Reading::Failed(reason) = decoded.outcome else {
        return Err("lost failure".into());
    };
    assert_eq!(reason.as_str(), "unknown synthetic test");
    let mut invalid = encoded;
    invalid["config_digest"] = serde_json::to_value(digest(b"wrong"))?;
    assert!(
        serde_json::from_value::<Interpretation>(invalid)
            .map(|_| ())
            .expect_err("invalid evidence must be refused")
            .is_data()
    );
    Ok(())
}

// Safety rule 2 (ADR 0003): parked items are kept as stored, so each kind keeps
// its exact spelling, and every spelling reads back to the same kind.
#[test]
fn parked_kinds_keep_their_stored_spelling() {
    let expected = [
        (ParkedKind::MappingFinding, "mapping_finding"),
        (ParkedKind::InterpretationFailure, "interpretation_failure"),
        (ParkedKind::ResultReconciliation, "result_reconciliation"),
        (ParkedKind::IncompleteTransfer, "incomplete_transfer"),
        (ParkedKind::DeliveryRejected, "parked"),
        (ParkedKind::DeliveryConflict, "conflict"),
        (ParkedKind::OrderVersionRefused, "order_version_refused"),
        (ParkedKind::CancelConflict, "law_cancel_refusal_conflict"),
        (ParkedKind::PollIntent, "poll_intent"),
    ];
    assert_eq!(ParkedKind::ALL.len(), expected.len());
    for ((kind, spelling), listed) in expected.into_iter().zip(ParkedKind::ALL) {
        // A new kind fails to compile here until it is listed above and in `ALL`.
        match kind {
            ParkedKind::MappingFinding
            | ParkedKind::InterpretationFailure
            | ParkedKind::ResultReconciliation
            | ParkedKind::IncompleteTransfer
            | ParkedKind::DeliveryRejected
            | ParkedKind::DeliveryConflict
            | ParkedKind::OrderVersionRefused
            | ParkedKind::CancelConflict
            | ParkedKind::PollIntent => {}
        }
        assert_eq!(kind, listed);
        assert_eq!(kind.as_str(), spelling);
        assert_eq!(ParkedKind::from_stored(spelling), Some(kind));
    }
    assert_eq!(ParkedKind::from_stored("future_kind"), None);
    assert_eq!(ParkedKind::from_stored("Parked"), None);
}
