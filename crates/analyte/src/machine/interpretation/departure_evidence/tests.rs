//! Tests ADR 0015 durable provenance, contract-v1 parking and strict source retention.
use super::super::tests::{FixtureError, Space, program};
use super::super::*;
use crate::Error;
use analyte_fixtures::ConfigEdit as _;
use analyte_model::ReceiptId;

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
// ADR 0015; HL7 §7.4.1.25; App contract v1 (ADR 0008)
#[tokio::test]
async fn documented_final_parks_with_ledger_and_no_outbox() -> Result<(), Error> {
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

// ADR 0015; CLSI LIS01-A2 §6.3.1.1; CLSI LIS02-A2
#[tokio::test]
async fn packing_provenance_survives_record_parse_failure() -> Result<(), Error> {
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

// ADR 0015; CLSI LIS01-A2 §6.3.1.1; App contract v1 (ADR 0008)
#[tokio::test]
async fn packed_results_deliver_without_changing_meaning() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let definition=include_str!("../../../../../core/config/examples/astm.toml")
        .rewrite("kind = \"serial\"","kind = \"tcp_client\"")
        .rewrite("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n","")
        .rewrite("deviations = []",&declaration("astm_packed_records_in_frame","packed_records",false));
    let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    let destination = crate::orders::destination(machine)?;
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

async fn latin1_receipt(encoding: &str) -> Result<(bool, Vec<u8>, String), Error> {
    let temp = tempfile::tempdir()?;
    let definition=include_str!("../../../../../core/config/examples/hl7.toml").rewrite("tcp_server","tcp_client")
        .rewrite("encoding = \"ascii\"",&format!("encoding = \"{encoding}\""))
        .rewrite("qc = []","qc = [{field={group='order',record='OBR',field=3,component=1,repetition=1},value='00012',classification='patient'}]");
    let program = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    let destination = crate::orders::destination(machine)?;
    // MSH-18 is empty; OBX-6 holds Latin-1 0xB5 (micro sign).
    let payload=b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|\xb5g/L|||||F\r";
    let raw = analyte_framing::hl7::encode(payload, analyte_framing::hl7::Limit::new(1048576)?)?;
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    let delivered = intake::interpret(&store, machine, id).await?;
    let summary = store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert_eq!(s.raw(id)?.as_deref(), Some(raw.as_slice()));
            let interpretation = s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?;
            let ledger = interpretation.departures.ok_or(FixtureError::Corruption)?;
            let notes = ledger
                .applied
                .as_slice()
                .iter()
                .filter(|e| e.meaning == analyte_model::DepartureMeaning::CharsetFromDefinition)
                .map(|e| {
                    format!(
                        "{} {} {}",
                        e.rule.as_str(),
                        e.source.as_str(),
                        e.declaration.as_str()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let payload = s
                .claim_destination(1, &destination)?
                .map(|claim| claim.payload)
                .unwrap_or_default();
            let parked = s
                .parked(0)?
                .iter()
                .filter_map(|row| row.details().ok())
                .map(|e| e.reason.as_str().to_owned())
                .collect::<Vec<_>>()
                .join("\n");
            Ok::<_, FixtureError>((payload, format!("{notes}\n{parked}")))
        })
        .await?;
    store.shutdown(deadline()?).await?;
    Ok((delivered, summary.0, summary.1))
}

// Deliberate tolerance: an empty MSH-18 uses the machine definition's character set.
// HL7 2.5.1 §2.15.9.18
#[tokio::test]
async fn empty_msh18_uses_definition_charset_with_note() -> Result<(), Error> {
    let (delivered, payload, evidence) = latin1_receipt("latin1").await?;
    assert!(delivered, "{evidence}");
    assert!(evidence.contains("hl7_charset_from_definition"));
    assert!(evidence.contains("encoding = \"latin1\""));
    let wire: serde_json::Value = serde_json::from_slice(&payload)?;
    let units = wire
        .pointer("/results/0/parts/0/units/original")
        .and_then(serde_json::Value::as_str);
    assert_eq!(units, Some("\u{b5}g/L"));
    // Without a declared character set the receipt is still refused, with the reason.
    let (delivered, payload, evidence) = latin1_receipt("ascii").await?;
    assert!(!delivered);
    assert!(payload.is_empty());
    assert!(!evidence.contains("hl7_charset_from_definition"));
    assert!(evidence.contains("no character set in the machine definition"));
    Ok(())
}

async fn zoneless_law_result(time_zone: Option<&str>) -> Result<(Vec<u8>, String, String), Error> {
    let temp = tempfile::tempdir()?;
    let definition = include_str!("../../../../../core/config/examples/law.toml")
        .rewrite("tcp_server", "tcp_client");
    let path = analyte_fixtures::write_machine(temp.path(), &definition, "127.0.0.1:5600")?;
    if let Some(zone) = time_zone {
        let site = std::fs::read_to_string(temp.path().join("site.toml"))?;
        std::fs::write(
            temp.path().join("site.toml"),
            site.rewrite(
                "retention = \"keep\"",
                &format!("retention = \"keep\"\ntime_zone = \"{zone}\""),
            ),
        )?;
    }
    let program = crate::installation::config::Program::load(&path)?;
    let machine = program.machines().first().ok_or("machine")?;
    let mut store = crate::runtime::service::open_store(&program, Box::new(Space)).await?;
    // MSH-7 has seconds but no UTC offset, as the IHE Gazelle simulator sends it.
    let payload = b"MSH|^~\\&|A|AF|M|MF|20261115133437||OUL^R22^OUL_R22|result-1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||00012a^LAB\rOBR||awos-1||SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|1|NM|SYN^Synthetic^99LAB|1|1|mg/L^mg/L^UCUM||N^Normal^HL70078|||F|||||operator||instrument^99LAB~module^99LAB|20261115120000||||||||||RSLT\r";
    let raw = analyte_framing::hl7::encode(payload, analyte_framing::hl7::Limit::new(1048576)?)?;
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    intake::interpret(&store, machine, id).await?;
    let found = store
        .request::<_, FixtureError>(deadline()?, move |s| {
            let kept = s.raw(id)?.unwrap_or_default();
            let interpretation = s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?;
            let ledger = interpretation.departures.ok_or(FixtureError::Corruption)?;
            let notes = ledger
                .applied
                .as_slice()
                .iter()
                .filter(|e| e.meaning == analyte_model::DepartureMeaning::ZoneFromSite)
                .map(|e| {
                    format!(
                        "{} | {} | {} | {}",
                        e.rule.as_str(),
                        e.standard.as_str(),
                        e.declaration.as_str(),
                        e.source.as_str()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let outcome = match interpretation.outcome {
                analyte_store::Reading::Failed(reason) => reason.as_str().to_owned(),
                analyte_store::Reading::Results(_) => "results".to_owned(),
            };
            Ok::<_, FixtureError>((kept, notes, outcome))
        })
        .await?;
    store.shutdown(deadline()?).await?;
    assert_eq!(found.0, raw);
    Ok(found)
}

// Deliberate tolerance: a LAW MSH-7 without an offset is read in the site zone.
// IHE PaLM TF-2b §C.4
#[tokio::test]
async fn zoneless_msh7_uses_site_zone_with_note() -> Result<(), Error> {
    let (_, notes, outcome) = zoneless_law_result(Some("Europe/Berlin")).await?;
    assert!(!outcome.contains("MSH-7"), "{outcome}");
    assert!(notes.contains("law_header_zone_from_site"), "{notes}");
    assert!(
        notes.contains("site time_zone = \"Europe/Berlin\""),
        "{notes}"
    );
    assert!(
        notes.contains("MSH-7 20261115133437 has no UTC offset"),
        "{notes}"
    );
    assert!(notes.contains("offset +01"), "{notes}");
    // Without a site zone the header is still refused, and no note is written.
    let (_, notes, outcome) = zoneless_law_result(None).await?;
    assert!(notes.is_empty(), "{notes}");
    assert!(outcome.contains("MSH-7"), "{outcome}");
    Ok(())
}
