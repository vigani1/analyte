//! Tests interpretation behavior with synthetic fixtures.
use super::*;
use crate::installation::config::Program;
use crate::intake::ByteReason;
use analyte_fixtures::ConfigEdit as _;
use analyte_model::{Identifier, Reason};
use analyte_store::{Attempt, Audit, RevisionOutcome};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub(super) enum FixtureError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Intake(#[from] analyte_store::IntakeError),
    #[error(transparent)]
    Operator(#[from] analyte_store::OperatorError),
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
    #[error("stored evidence is corrupt")]
    Corruption,
    #[error("receipt does not exist")]
    UnknownReceipt,
}

pub(super) use analyte_fixtures::UnlimitedSpace as Space;

fn action(receipt: ReceiptId, parked: Option<i64>) -> Result<Reprocess, Error> {
    Ok(Reprocess {
        receipt,
        parked,
        audit: Audit {
            actor: Identifier::new("synthetic-operator")?,
            action: Identifier::new("receipt-reprocess")?,
            targets: Vec::new(),
            time: 50,
            reason: Reason::new("mapping repaired")?,
        },
    })
}

pub(super) fn program(root: &Path, definition: &str, endpoint: &str) -> Result<Program, Error> {
    std::fs::write(root.join("machine.toml"), definition)?;
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../../core/config/examples/site.toml")
            .rewrite("127.0.0.1:5600", endpoint),
    )?;
    let path = root.join("program.toml");
    std::fs::write(
        &path,
        "schema_version = 1\ndata_directory = 'data'\n[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n",
    )?;
    Program::load(&path).map_err(Into::into)
}

#[tokio::test]
async fn plan11_adr0003_mapping_repaired_once_retransmission_conflict_and_original_retained_rule8_no_machine_io()
-> Result<(), Error> {
    let hl7 = include_str!("../../../../core/config/examples/hl7.toml")
        .rewrite("tcp_server", "tcp_client");
    let raw = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";
    let raw = analyte_framing::hl7::encode(raw, analyte_framing::hl7::Limit::new(1048576)?)?;
    exercise(&hl7, raw).await?;
    let astm = include_str!("../../../../core/config/examples/astm.toml")
        .rewrite("kind = \"serial\"", "kind = \"tcp_client\"")
        .rewrite("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "")
        .rewrite("transport = \"serial\"", "transport = \"tcp_client\"");
    let payload = b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";
    let mut wire = Vec::new();
    let mut number = analyte_framing::astm::FrameNumber::FIRST;
    // LIS01 sections 6/8: multiple records and split ETB frames, retaining all wire bytes.
    for record in payload.split_inclusive(|b| *b == b'\r') {
        for frame in
            analyte_framing::astm::encode(record, number, analyte_framing::astm::Limit::new(32)?)?
        {
            wire.extend(frame);
            number = number.next();
        }
    }
    exercise(&astm, wire).await
}

async fn exercise(definition: &str, raw: Vec<u8>) -> Result<(), Error> {
    let temp = crate::admin::private_tempdir()?;
    let receiver = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = receiver.local_addr()?.to_string();
    let missing = definition.rewrite("tests = [{machine_code=\"SYN\", machine_system=\"synthetic\", app_code=\"DEMO\", app_system=\"demo\"}]",
        "tests = []");
    let old = program(temp.path(), &missing, &endpoint)?;
    let mut store = crate::runtime::service::open_store(&old, Box::new(Space)).await?;
    let machine = old.machines().first().ok_or("missing machine")?;
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    assert!(!intake::interpret(&store, machine, id).await?);
    let (parked, original) = store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert!(s.deliveries(0)?.is_empty());
            let parked = s.parked(0)?.first().ok_or(FixtureError::Corruption)?.id;
            let original =
                serde_json::to_string(&s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?)?;
            Ok::<_, FixtureError>((parked, original))
        })
        .await?;
    assert_repeated_failure_stays_parked(&store, machine, id, parked).await?;
    let current = program(temp.path(), definition, &endpoint)?;
    let machine = current.machines().first().ok_or("missing machine")?;
    let commit = reprocess(&store, machine, action(id, Some(parked))?).await?;
    assert_eq!(commit.version, 3);
    assert!(matches!(
        commit.outcomes.as_slice(),
        [RevisionOutcome::New(_)]
    ));
    assert_repair_resolves_original(&store, id, parked, original).await?;
    assert_retransmission_and_conflict(
        &store,
        machine,
        id,
        raw,
        temp.path(),
        definition,
        &endpoint,
    )
    .await?;
    // Even with a reachable configured machine endpoint, reprocess opens no transport.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), receiver.accept())
            .await
            .is_err()
    );
    store.shutdown(deadline()?).await?;
    Ok(())
}

async fn assert_repeated_failure_stays_parked(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    parked: i64,
) -> Result<(), Error> {
    // Repeated failure appends a version and new reason, leaving the original item open.
    reprocess(store, machine, action(id, Some(parked))?).await?;
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert!(
                s.parked_item(parked)?
                    .ok_or(FixtureError::Corruption)?
                    .resolution
                    .is_none()
            );
            assert!(s.parked(0)?.len() >= 2);
            assert_eq!(s.audits(0)?.len(), 1);
            Ok::<_, FixtureError>(())
        })
        .await?;
    Ok(())
}

async fn assert_repair_resolves_original(
    store: &Handle,
    id: ReceiptId,
    parked: i64,
    original: String,
) -> Result<(), Error> {
    let original_check = original;
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert_eq!(
                serde_json::to_string(&s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?)?,
                original_check
            );
            let resolved = s
                .parked_item(parked)?
                .ok_or(FixtureError::Corruption)?
                .resolution
                .ok_or(FixtureError::Corruption)?;
            assert!(resolved.contains("interpretation:"));
            assert!(resolved.contains(":3"));
            assert_eq!(s.deliveries(0)?.len(), 1);
            let claim = s.claim(50)?.ok_or(FixtureError::Corruption)?;
            Ok(s.attempted(claim.id, &Attempt::Done, 50)?)
        })
        .await?;
    Ok(())
}

async fn assert_retransmission_and_conflict(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    raw: Vec<u8>,
    root: &Path,
    definition: &str,
    endpoint: &str,
) -> Result<(), Error> {
    let commit = reprocess(store, machine, action(id, None)?).await?;
    assert!(matches!(
        commit.outcomes.as_slice(),
        [RevisionOutcome::Retransmission(_)]
    ));
    let changed = program(
        root,
        &definition.rewrite("app_code=\"DEMO\"", "app_code=\"CHANGED\""),
        endpoint,
    )?;
    let machine = changed.machines().first().ok_or("missing machine")?;
    let commit = reprocess(store, machine, action(id, None)?).await?;
    assert_eq!(commit.outcomes, vec![RevisionOutcome::Conflict]);
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            let receipt = s.receipt(id)?.ok_or(FixtureError::Corruption)?;
            assert_eq!(receipt.raw, raw);
            assert_eq!(
                receipt.config,
                s.interpretation(id, 1)?
                    .ok_or(FixtureError::Corruption)?
                    .config
            );
            let current = s.interpretation(id, 5)?.ok_or(FixtureError::Corruption)?;
            let json = serde_json::to_value(&current)?;
            assert_eq!(
                json["config_digest"],
                serde_json::to_value(analyte_model::Digest(analyte_contract::content_digest(
                    &current.config
                )))?
            );
            assert_ne!(current.config, receipt.config);
            assert_eq!(s.deliveries(0)?.len(), 1);
            assert!(s.claim(50)?.is_none());
            assert_eq!(s.audits(0)?.len(), 4);
            assert!(s.parked(0)?.iter().any(|p| p.evidence.contains("Conflict")));
            Ok::<_, FixtureError>(())
        })
        .await?;
    Ok(())
}

use analyte_fixtures::poct as poct_peer;

#[tokio::test]
async fn plan11_rule3_vendor_receipts_use_live_interpretation_and_rule8_stay_parked_without_machine_io()
-> Result<(), Error> {
    use analyte_autolyser::syntax::{Field, List, PatientCode, Report, Sample, Value};
    let report = Report::new(
        &PatientCode::from_eight(b"00012aBC")?,
        List::Routine,
        Sample::Serum,
        None,
        &[Value {
            code: Field::new(b"SYN ")?,
            text: Field::new(b"<0.1   ")?,
        }],
    )?
    .raw;
    let report = analyte_framing::autolyser::finish(report);
    let poct = poct_peer::observation("observation", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
    analyte_poct::Message::parse(poct.as_bytes(), poct_peer::limits())?;
    let cases = [
        (
            include_str!("../../../../core/config/examples/vidas_literal.toml"),
            None,
        ),
        (
            include_str!("../../../../core/config/examples/autolyser.toml"),
            Some(report),
        ),
        (
            include_str!("../../../../core/config/examples/poct.toml"),
            Some(poct.into_bytes()),
        ),
    ];
    for (definition, wire) in cases {
        assert_vendor_receipt_stays_parked(definition, wire).await?;
    }
    Ok(())
}

// ADR 0003 / AGENTS.md safety rule 3: original and reprocessed config provenance is retained.
#[tokio::test]
async fn adr0003_rule3_effective_configs_have_distinct_recorded_versions() -> Result<(), Error> {
    let temp = crate::admin::private_tempdir()?;
    let definition = include_str!("../../../../core/config/examples/hl7.toml")
        .rewrite("tcp_server", "tcp_client");
    let old = program(temp.path(), &definition, "127.0.0.1:5600")?;
    let mut store = crate::runtime::service::open_store(&old, Box::new(Space)).await?;
    let original = old.machines().first().ok_or("missing machine")?;
    let id = ReceiptId::new();
    let connection = intake::ConnectionEvidence::new(original)?;
    intake::receive(&store, &connection, id, b"unparseable original".to_vec()).await?;
    intake::failed_reading(&store, original, id, "synthetic failure").await?;
    let changed = program(temp.path(), &definition, "127.0.0.1:5601")?;
    let current = changed.machines().first().ok_or("missing machine")?;
    intake::park_bytes(
        &store,
        &intake::ConnectionEvidence::new(current)?,
        b"unparseable changed".to_vec(),
        ByteReason::Synthetic,
    )
    .await?;
    reprocess(&store, current, action(id, None)?).await?;
    let old_config = original.snapshot.canonical().as_bytes().to_vec();
    let new_config = current.snapshot.canonical().as_bytes().to_vec();
    let old_digest = original.snapshot.digest();
    let new_digest = current.snapshot.digest();
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            let receipt = s.receipt(id)?.ok_or(FixtureError::UnknownReceipt)?;
            let first = s
                .interpretation(id, 1)?
                .ok_or(FixtureError::UnknownReceipt)?;
            let second = s
                .interpretation(id, 2)?
                .ok_or(FixtureError::UnknownReceipt)?;
            assert_eq!(receipt.config, old_config);
            assert_eq!(receipt.metadata.config_digest.0, old_digest);
            assert_eq!(first.config_version, receipt.metadata.config_version);
            assert_eq!(first.config, old_config);
            assert_eq!(second.config, new_config);
            assert_ne!(first.config_version, second.config_version);
            assert_eq!(
                second.config_version.as_str(),
                new_digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            for summary in s.receipts("")? {
                let parked = s.receipt(summary.id)?.ok_or(FixtureError::UnknownReceipt)?;
                if parked.raw == b"unparseable changed" {
                    assert_eq!(parked.metadata.config_version, second.config_version);
                    assert_eq!(parked.metadata.config_digest.0, new_digest);
                    assert_eq!(
                        s.interpretation(summary.id, 1)?
                            .ok_or(FixtureError::UnknownReceipt)?
                            .config_version,
                        second.config_version
                    );
                }
            }
            Ok::<_, FixtureError>(())
        })
        .await?;
    store.shutdown(deadline()?).await?;
    Ok(())
}

// ADR 0004: site policy becomes destination Limits, rather than a host constant.
#[test]
fn rule9_destination_config_sets_operational_freshness_and_rejects_zero() -> Result<(), Error> {
    let temp = crate::admin::private_tempdir()?;
    let definition = include_str!("../../../../core/config/examples/hl7.toml")
        .rewrite("tcp_server", "tcp_client");
    let loaded = program(temp.path(), &definition, "127.0.0.1:5600")?;
    assert_eq!(
        loaded.machines()[0].destination_limits.freshness.as_secs(),
        30
    );
    let site = temp.path().join("site.toml");
    let original = std::fs::read_to_string(&site)?;
    for seconds in [7, 120] {
        std::fs::write(
            &site,
            format!("{original}\nfreshness_seconds = {seconds}\n"),
        )?;
        let configured = Program::load(&temp.path().join("program.toml"))?;
        assert_eq!(
            configured.machines()[0]
                .destination_limits
                .freshness
                .as_secs(),
            seconds
        );
    }
    std::fs::write(&site, format!("{original}\nfreshness_seconds = 0\n"))?;
    assert!(Program::load(&temp.path().join("program.toml")).is_err());
    Ok(())
}

async fn assert_vendor_receipt_stays_parked(
    definition: &str,
    wire: Option<Vec<u8>>,
) -> Result<(), Error> {
    let temp = crate::admin::private_tempdir()?;
    let peer = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let current = program(
        temp.path(),
        &definition.rewrite("tcp_server", "tcp_client"),
        &peer.local_addr()?.to_string(),
    )?;
    let machine = current.machines().first().ok_or("missing machine")?;
    let raw = match wire {
        Some(wire) => wire,
        None => vidas_config(machine)?
            .encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn<0.1|y3mg/L|qd1:10|")?,
    };
    let mut store = crate::runtime::service::open_store(&current, Box::new(Space)).await?;
    let id = ReceiptId::new();
    intake::receive(
        &store,
        &intake::ConnectionEvidence::new(machine)?,
        id,
        raw.clone(),
    )
    .await?;
    assert!(!intake::interpret(&store, machine, id).await?);
    let parked = store.request::<_, FixtureError>(deadline()?, move |s| {
        let version = s.interpretation(id, 1)?.ok_or(FixtureError::Corruption)?;
        assert!(matches!(version.outcome, analyte_store::Reading::Results(results) if !results.is_empty()));
        Ok::<_, FixtureError>(s.parked(0)?.first().ok_or(FixtureError::Corruption)?.id)
    }).await?;
    let commit = reprocess(&store, machine, action(id, Some(parked))?).await?;
    assert_eq!(commit.outcomes, vec![RevisionOutcome::Unresolved]);
    store
        .request::<_, FixtureError>(deadline()?, move |s| {
            assert_eq!(s.receipt(id)?.ok_or(FixtureError::Corruption)?.raw, raw);
            assert!(s.interpretation(id, 1)?.is_some());
            assert!(s.interpretation(id, 2)?.is_some());
            assert!(
                s.parked_item(parked)?
                    .ok_or(FixtureError::Corruption)?
                    .resolution
                    .is_none()
            );
            assert_eq!(s.audits(0)?.len(), 1);
            assert!(s.deliveries(0)?.is_empty());
            Ok(())
        })
        .await?;
    assert_poct_controls_stay_failed(&store, machine).await?;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), peer.accept())
            .await
            .is_err()
    );
    store.shutdown(deadline()?).await?;
    Ok(())
}

async fn assert_poct_controls_stay_failed(store: &Handle, machine: &Machine) -> Result<(), Error> {
    if matches!(machine.snapshot.effective().stack(), Stack::Poct(_)) {
        // Control receipts are retained too; reprocessing them never becomes a
        // machine query or an invented empty result success (rule 9).
        for control in [poct_peer::hello("hello"), poct_peer::status("status", "1")] {
            let id = ReceiptId::new();
            intake::receive(
                store,
                &intake::ConnectionEvidence::new(machine)?,
                id,
                control.into_bytes(),
            )
            .await?;
            reprocess(store, machine, action(id, None)?).await?;
            store
                .request::<_, FixtureError>(deadline()?, move |s| {
                    assert!(matches!(
                        s.interpretation(id, 1)?
                            .ok_or(FixtureError::Corruption)?
                            .outcome,
                        analyte_store::Reading::Failed(_)
                    ));
                    Ok::<_, FixtureError>(())
                })
                .await?;
        }
    }
    Ok(())
}
