//! Tests uncertain ASTM send outcomes across restart and operator reconciliation.
use super::order_cases::{configure_orders, machine_state};
use super::*;
use crate::support::step;

#[tokio::test]
async fn adr0004_rule8_astm_send_timeout_is_uncertain_and_restart_never_resends()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), true)?;
    let definition = temp.path().join("astm.toml");
    std::fs::write(
        &definition,
        std::fs::read_to_string(&definition)?
            .replace("send_timeout_ms = 15000", "send_timeout_ms = 50").replace("deviations = []", "deviations = [{name=\"synthetic-fast-timeout\",key=\"dialog.send_timeout_ms\",reason=\"Shortened loopback failure-path timer\"}]"),
    )?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = step(
        "start initial service",
        Running::start(&path, &reference.url, "synthetic-api"),
    )
    .await?;
    let mut peer = step("connect initial ASTM peer", connect(&path, "astm")).await?;
    assert_eq!(step("receive initial ENQ", peer.read_u8()).await?, 5);
    step(
        "commit initial Uncertain",
        machine_state(&path, "Uncertain", 1),
    )
    .await?;
    step("deliver Uncertain", wait_state(&path, "done", 1)).await?;
    assert_eq!(receiver_count(&receiver_path)?, 1);
    drop(peer);
    step("stop initial service", service.stop()).await?;
    let service = step(
        "restart uncertain service",
        Running::start(&path, &reference.url, "synthetic-api"),
    )
    .await?;
    let mut peer = step("connect restarted ASTM peer", connect(&path, "astm")).await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    step(
        "retain Uncertain after restart",
        machine_state(&path, "Uncertain", 1),
    )
    .await?;
    drop(peer);
    step("stop restarted service", service.stop()).await?;
    let id: i64 = database(&path)?.query_row("SELECT id FROM machine_orders", [], |r| r.get(0))?;
    reconcile(&path, id).await?;
    step("commit reconciliation", machine_state(&path, "Accepted", 1)).await?;
    let audits: Vec<String> = database(&path)?.prepare("SELECT evidence FROM audit WHERE evidence LIKE '%synthetic-operator%' ORDER BY sequence")?.query_map([],|r|r.get(0))?.collect::<Result<_,_>>()?;
    assert_eq!(audits.len(), 2);
    assert!(audits.first().is_some_and(|s| s.contains("orders-list")));
    assert!(
        audits
            .last()
            .is_some_and(|s| s.contains("verified synthetic work list"))
    );
    let service = step(
        "start reconciled service",
        Running::start(&path, &reference.url, "synthetic-api"),
    )
    .await?;
    step("deliver reconciliation", wait_state(&path, "done", 2)).await?;
    assert_eq!(receiver_count(&receiver_path)?, 2);
    step("stop reconciled service", service.stop()).await?;
    step("close order receiver", reference.close()).await?;
    Ok(())
}

async fn reconcile(path: &Fixture, id: i64) -> Result<(), Error> {
    for (name, arguments) in [
        (
            "list uncertain orders CLI",
            vec!["orders".into(), "list".into()],
        ),
        (
            "reconcile uncertain order CLI",
            vec![
                "orders".into(),
                "reconcile".into(),
                id.to_string(),
                "--as".into(),
                "accepted".into(),
                "--reason".into(),
                "verified synthetic work list".into(),
            ],
        ),
    ] {
        let output = step(
            name,
            tokio::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
                .kill_on_drop(true)
                .arg("--config")
                .arg(path.as_ref())
                .arg("--actor")
                .arg("synthetic-operator")
                .args(arguments)
                .output(),
        )
        .await?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
