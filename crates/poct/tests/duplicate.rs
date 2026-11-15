//! Appendix B section 3.2 and ADR 0003 observation identity tests.
mod peer_messages;
use analyte_model::{
    Identifier, IdentityStatus, NamespacedId, ObservationIdentity, Reason, ResultIdentity,
    Revision, SourceText,
};
use analyte_poct::{
    DuplicateIndex, DuplicateStatus, Message, ObservationEvidence, ObservationKey,
    RecordedObservation, observation_evidence,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn key(run: &str, revision: u64) -> Result<ObservationKey, analyte_model::TextError> {
    // This is synthetic host reconciliation evidence, not a key inferred from
    // a POCT sequence number, value, patient name or receipt identifier.
    Ok(ObservationKey {
        device: Identifier::new("synthetic-device")?,
        result: ResultIdentity {
            observation: ObservationIdentity {
                code: NamespacedId {
                    namespace: Identifier::new("synthetic-system")?,
                    value: Identifier::new("synthetic-analyte")?,
                },
                sub_id: SourceText::new("")?,
                run: SourceText::new(run)?,
                order: NamespacedId {
                    namespace: Identifier::new("synthetic-orders")?,
                    value: Identifier::new("trusted-order")?,
                },
            },
            revision: Revision(revision),
        },
    })
}

fn evidence(id: &str, value: &str) -> Result<ObservationEvidence, Box<dyn std::error::Error>> {
    let raw = peer_messages::observation(id, true, &format!("<OBS.value V=\"{value}\" U=\"unit\"/>"));
    let message = Message::parse(raw.as_bytes(), peer_messages::limits())?;
    observation_evidence(&message)?
        .into_iter()
        .next()
        .ok_or_else(|| "no observation".into())
}

#[test]
fn poct01_b_3_2_duplicate_is_independent_of_receipt_and_delivery() -> TestResult {
    let mut index = DuplicateIndex::new(10, 65536)?;
    let key = key("trusted-run-A", 0)?;
    assert_eq!(
        index.record(RecordedObservation {
            key: key.clone(),
            evidence: evidence("receipt-A", "1.20")?
        })?,
        DuplicateStatus::New
    );
    assert_eq!(
        index.check(
            &key.device,
            &IdentityStatus::Resolved(key.result.clone()),
            &evidence("receipt-B", "1.20")?
        ),
        DuplicateStatus::Duplicate
    );
    assert_eq!(
        index.check(
            &key.device,
            &IdentityStatus::Resolved(key.result.clone()),
            &evidence("receipt-C", "2.50")?
        ),
        DuplicateStatus::Conflict
    );
    assert_eq!(index.entries().len(), 1);
    Ok(())
}

#[test]
fn poct01_b_3_2_correction_rerun_and_unresolved_are_distinct() -> TestResult {
    let mut index = DuplicateIndex::new(10, 65536)?;
    let original = key("trusted-run-A", 0)?;
    let evidence = evidence("receipt", "1.20")?;
    index.record(RecordedObservation {
        key: original.clone(),
        evidence: evidence.clone(),
    })?;
    // A corrected revision is supplied by the store after it has established
    // correction evidence and changed content; a rerun has its own run key.
    for (different, content) in [
        (
            key("trusted-run-A", 1)?,
            crate::evidence("corrected-receipt", "2.50")?,
        ),
        (key("trusted-run-B", 0)?, evidence.clone()),
    ] {
        assert_eq!(
            index.check(
                &different.device,
                &IdentityStatus::Resolved(different.result),
                &content
            ),
            DuplicateStatus::New
        );
    }
    assert_eq!(
        index.check(
            &original.device,
            &IdentityStatus::Unresolved(Reason::new("source cannot identify the run")?),
            &evidence
        ),
        DuplicateStatus::Unresolved
    );
    Ok(())
}

#[test]
fn poct01_b_3_2_lost_ack_reconnect_and_restart_restore_same_identity() -> TestResult {
    let mut first = DuplicateIndex::new(1, 65536)?;
    first.record(RecordedObservation {
        key: key("trusted-run", 0)?,
        evidence: evidence("before-crash", "1.20")?,
    })?;
    let durable = first.entries().to_vec();
    let mut restarted = DuplicateIndex::new(1, 65536)?;
    for record in durable {
        restarted.record(record)?;
    }
    let key = key("trusted-run", 0)?;
    assert_eq!(
        restarted.check(
            &key.device,
            &IdentityStatus::Resolved(key.result),
            &evidence("after-reconnect", "1.20")?
        ),
        DuplicateStatus::Duplicate
    );
    assert!(
        restarted
            .record(RecordedObservation {
                key: crate::key("new-run", 0)?,
                evidence: evidence("new", "1.20")?
            })
            .is_err()
    );
    assert_eq!(restarted.entries().len(), 1);
    for raw in [peer_messages::hello("h"), peer_messages::status("s", "0")] {
        Message::parse(raw.as_bytes(), peer_messages::limits())?;
    }
    Ok(())
}

#[test]
fn poct01_b_3_2_resend_reason_and_attribute_order_are_not_new_results() -> TestResult {
    let raw = peer_messages::observation("receipt", true, "<OBS.value V=\"1.20\" U=\"unit\"/>");
    let retransmitted = raw
        .replace("<SVC>", "<SVC>\n ")
        .replace("V=\"1.20\" U=\"unit\"", "U=\"unit\" V=\"1.20\"")
        .replace(
            "<SVC.sequence_nbr",
            "<SVC.reason_cd V=\"RES\"/><SVC.sequence_nbr",
        );
    assert_eq!(
        observation_evidence(&Message::parse(raw.as_bytes(), peer_messages::limits())?)?,
        observation_evidence(&Message::parse(retransmitted.as_bytes(), peer_messages::limits())?)?
    );
    Ok(())
}
