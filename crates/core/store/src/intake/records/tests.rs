//! Tests parsed record storage and receipt associations.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn rule3_interpretation_digest_verified_legacy_evidence_still_reads() -> TestResult {
    let evidence = interpretation()?;
    let mut encoded = serde_json::to_value(&evidence)?;
    encoded["config_digest"] = serde_json::to_value(digest(b"different"))?;
    assert!(serde_json::from_value::<Interpretation>(encoded.clone()).is_err());
    encoded
        .as_object_mut()
        .ok_or("not object")?
        .remove("config_digest");
    let legacy: Interpretation = serde_json::from_value(encoded)?;
    assert_eq!(legacy.config, evidence.config);
    Ok(())
}

#[test]
fn mutation_adr0003_record_interpretation_roundtrip_keeps_provenance_and_failure_reason()
-> TestResult {
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Failed(Reason::new("unknown synthetic test")?);
    let encoded = serde_json::to_value(&evidence)?;
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
    assert!(serde_json::from_value::<Interpretation>(invalid).is_err());
    Ok(())
}
