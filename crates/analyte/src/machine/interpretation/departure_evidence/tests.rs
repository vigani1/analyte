//! Tests ADR 0015 durable provenance, contract-v1 parking and strict source retention.
use super::super::tests::{FixtureError, Space, program};
use super::super::*;
use analyte_fixtures::ConfigEdit as _;

fn declaration(rule: &str, guarantee: &str, hl7: bool) -> String {
    let mut text = include_str!("../../../../../core/config/tests/fixtures/departure.toml")
        .rewrite("astm_frame_limit", rule)
        .rewrite("compatibility", guarantee);
    if hl7 {
        text = text
            .rewrite("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v251\"")
            .rewrite("astm_results", "oru_r01_patient");
    }
    text
}
#[tokio::test]
async fn adr0015_hl7_s74125_v1_parks_with_ledger_original_and_no_outbox() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let declaration = declaration("hl7_absent_obr25_means_final", "absent_obr25_final", true);
    let definition=include_str!("../../../../../core/config/examples/hl7.toml").rewrite("tcp_server","tcp_client")
        .rewrite("deviations = []",&declaration)
        .rewrite("qc = []","qc = [{field={group='order',record='OBR',field=3,component=1,repetition=1},value='00012',classification='patient'}]");
    let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    let payload=b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012|SYN^Synthetic^synthetic\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";
    let raw = analyte_framing::hl7::encode(payload, analyte_framing::hl7::Limit::new(1048576)?)?;
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    assert!(!intake::interpret(&store, machine, id).await?);
    store.request::<_, FixtureError>(deadline()?,move |s| {
        assert_eq!(s.raw(id)?.as_deref(),Some(raw.as_slice()));
        assert!(s.deliveries(0)?.is_empty());
        let interpretation=s.interpretation(id,1)?.ok_or(FixtureError::Corruption)?;
        let ledger=interpretation.departures.ok_or(FixtureError::Corruption)?;
        assert_eq!(ledger.enabled.as_slice().len(),1);
        assert_eq!(ledger.applied.as_slice().len(),1);
        assert_eq!(ledger.applied.as_slice().first().map(|e| e.meaning),Some(analyte_model::DepartureMeaning::FinalReportFromDocumentation));
        assert!(s.parked(0)?.iter().any(|row| row.details().is_ok_and(|e| e.reason.as_str().contains("departure-derived status requires a contract version that carries departure evidence"))));
        Ok::<_, FixtureError>(())
    }).await?;
    store.shutdown(deadline()?).await?;
    Ok(())
}

#[tokio::test]
async fn adr0015_lis01_s6311_packing_provenance_survives_lis02_failure() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let definition=include_str!("../../../../../core/config/examples/astm.toml")
        .rewrite("kind = \"serial\"","kind = \"tcp_client\"")
        .rewrite("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n","")
        .rewrite("deviations = []",&declaration("astm_packed_records_in_frame","packed_records",false));
    let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    let frames: Vec<_> = analyte_framing::astm::encode(
        b"H|bad\rR|bad-hierarchy\r",
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::default(),
    )?
    .collect();
    let raw = frames.concat();
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    assert!(!intake::interpret(&store, machine, id).await?);
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert_eq!(s.raw(id)?.as_deref(), Some(raw.as_slice()));
            let interpretation = s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?;
            assert!(matches!(
                interpretation.outcome,
                analyte_store::Reading::Failed(_)
            ));
            let ledger = interpretation.departures.ok_or(FixtureError::Corruption)?;
            assert_eq!(ledger.enabled.as_slice().len(), 1);
            assert_eq!(ledger.applied.as_slice().len(), 2);
            assert!(
                ledger
                    .applied
                    .as_slice()
                    .iter()
                    .all(|entry| !entry.spans.as_slice().is_empty())
            );
            assert!(s.deliveries(0)?.is_empty());
            Ok::<_, FixtureError>(())
        })
        .await?;
    store.shutdown(deadline()?).await?;
    Ok(())
}

#[tokio::test]
async fn adr0015_lis01_s6311_packed_results_use_v1_without_changing_source_meaning()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let definition=include_str!("../../../../../core/config/examples/astm.toml")
        .rewrite("kind = \"serial\"","kind = \"tcp_client\"")
        .rewrite("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n","")
        .rewrite("deviations = []",&declaration("astm_packed_records_in_frame","packed_records",false));
    let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    let destination = crate::orders::destination(machine);
    let payload=b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";
    let raw: Vec<_> = analyte_framing::astm::encode(
        payload,
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::default(),
    )?
    .flatten()
    .collect();
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    assert!(intake::interpret(&store, machine, id).await?);
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert_eq!(s.raw(id)?.as_deref(), Some(raw.as_slice()));
            let interpretation = s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?;
            let ledger = interpretation.departures.ok_or(FixtureError::Corruption)?;
            assert_eq!(ledger.applied.as_slice().len(), 5);
            let claim = s
                .claim_destination(1, &destination)?
                .ok_or(FixtureError::Corruption)?;
            assert_eq!(claim.contract_version, 1);
            let wire: serde_json::Value = serde_json::from_slice(&claim.payload)?;
            assert_eq!(
                wire.get("contract_version")
                    .and_then(serde_json::Value::as_u64),
                Some(1)
            );
            assert!(
                !String::from_utf8_lossy(&claim.payload).contains("astm_packed_records_in_frame")
            );
            Ok::<_, FixtureError>(())
        })
        .await?;
    store.shutdown(deadline()?).await?;
    Ok(())
}
