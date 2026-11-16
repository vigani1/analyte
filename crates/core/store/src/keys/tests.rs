//! Pins all durable key kinds to the existing storage bytes.
use super::*;
use analyte_model::{SampleId, SourceText};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn pin<T: Key + Serialize>(value: &T, golden: &str) -> TestResult {
    assert_eq!(key(value)?, golden);
    assert_eq!(serde_json::to_string(value)?, golden);
    Ok(())
}

#[test]
fn adr0003_durable_key_golden_bytes_and_decoding_round_trip() -> TestResult {
    let uuid = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000001")?;
    let receipt = ReceiptId::try_from(uuid)?;
    let delivery = DeliveryId::try_from(uuid)?;
    let golden = r#""01900000-0000-7000-8000-000000000001""#;
    pin(&receipt, golden)?;
    pin(&delivery, golden)?;
    assert_eq!(key(&serde_json::from_str::<ReceiptId>(golden)?)?, golden);
    assert_eq!(key(&serde_json::from_str::<DeliveryId>(golden)?)?, golden);
    let id = Identifier::new("M")?;
    let ns = NamespacedId {
        namespace: Identifier::new("ns")?,
        value: Identifier::new("00Ab")?,
    };
    let ns_json = r#"{"namespace":"ns","value":"00Ab"}"#;
    pin(&ns, ns_json)?;
    let obs = ObservationIdentity {
        code: ns.clone(),
        sub_id: SourceText::new("")?,
        run: SourceText::new("r\n\"")?,
        order: ns,
    };
    let obs_json = r#"{"code":{"namespace":"ns","value":"00Ab"},"sub_id":"","run":"r\n\"","order":{"namespace":"ns","value":"00Ab"}}"#;
    let identity_json = format!(r#"["M",{obs_json}]"#);
    pin(&(&id, &obs), &identity_json)?;
    let (machine, observation): (Identifier, ObservationIdentity) =
        serde_json::from_str(&identity_json)?;
    assert_eq!(key(&(&machine, &observation))?, identity_json);
    let ordering = crate::OrderingKey {
        machine: id.clone(),
        observation: obs,
    };
    let ordering_json = format!(r#"{{"machine":"M","observation":{obs_json}}}"#);
    pin(&ordering, &ordering_json)?;
    assert_eq!(
        key(&serde_json::from_str::<crate::OrderingKey>(&ordering_json)?)?,
        ordering_json
    );
    let command = crate::MachineOrderKey {
        destination: id.clone(),
        order: id.clone(),
        machine: id.clone(),
    };
    let command_json = r#"{"destination":"M","order":"M","machine":"M"}"#;
    pin(&command, command_json)?;
    assert_eq!(
        key(&serde_json::from_str::<crate::MachineOrderKey>(
            command_json
        )?)?,
        command_json
    );
    pin(&(&id, &id), r#"["M","M"]"#)?;
    pin(&(&"M".to_owned(), &"M".to_owned()), r#"["M","M"]"#)?;
    pin(&SourceText::new("r\n\"")?, r#""r\n\"""#)?;
    pin(&Some(&SampleId::new("00Ab")?), r#""00Ab""#)?;
    pin(&None::<&SampleId>, "null")?;
    for (alert, golden) in [
        (crate::AlertKey::OldestUndelivered, r#""OldestUndelivered""#),
        (
            crate::AlertKey::MachineSilent(id),
            r#"{"MachineSilent":"M"}"#,
        ),
        (crate::AlertKey::DiskReserve, r#""DiskReserve""#),
        (crate::AlertKey::OrderCacheStale, r#""OrderCacheStale""#),
        (crate::AlertKey::UncertainOrders, r#""UncertainOrders""#),
        (crate::AlertKey::ParkedItems, r#""ParkedItems""#),
        (crate::AlertKey::RestorePaused, r#""RestorePaused""#),
    ] {
        pin(&alert, golden)?;
        assert_eq!(
            key(&serde_json::from_str::<crate::AlertKey>(golden)?)?,
            golden
        );
    }
    Ok(())
}
