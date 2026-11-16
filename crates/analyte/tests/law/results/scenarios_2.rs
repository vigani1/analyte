//! Tests law tf2b c1 b7 second obx error reports actual occurrence; law adr0011 c7 all status moves and unverified delivery.
use super::*;

#[tokio::test]
async fn law_tf2b_c1_b7_second_obx_error_reports_actual_occurrence() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let first = result("second-obx", "\"\"", "run^ANALYZER", "1.0", "F");
    let obx = first.split("OBX|").nth(1).ok_or("synthetic OBX missing")?;
    let second = format!("OBX|{}", obx.replacen("1|NM|", "2|NM|", 1));
    for (payload, location) in [
        (
            format!("{first}{}", second.replace("|F|", "|D|")),
            "OBX^2^11",
        ),
        (
            format!("{first}{}", second.replacen("OBX|2|", "OBX|3|", 1)),
            "OBX^2^1",
        ),
        (
            format!("{}{second}", first.replace("|F|", "|D|")),
            "OBX^1^11",
        ),
    ] {
        let ack = hl7(&mut peer, payload.as_bytes()).await?;
        assert!(ack.contains("MSA|AE|second-obx"), "{ack}");
        assert!(ack.contains(&format!("ERR||{location}|")), "{ack}");
        assert_eq!(count(&path, "outbox")?, 0);
        assert_eq!(count(&path, "observation_revisions")?, 0);
        let raw: Vec<u8> = database(&path)?.query_row(
            "SELECT raw FROM receipts ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            raw,
            analyte_framing::hl7::encode(
                payload.as_bytes(),
                analyte_framing::hl7::Limit::new(1048576)?
            )?
        );
    }
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0011; PaLM TF-2b Rev 11 C.7 Figure C.7-1 p.111, Table C.7-18 p.110.
#[tokio::test]
async fn law_adr0011_c7_all_status_moves_and_unverified_delivery() -> Result<(), Error> {
    for before in ["P", "R", "F", "C", "X"] {
        for after in ["P", "R", "F", "C", "X"] {
            let temp = tempfile::tempdir()?;
            let path = vendor_config(temp.path(), "law", &definition())?;
            let reference = Reference::start(
                &temp.path().join("receiver.db"),
                TcpListener::bind("127.0.0.1:0").await?,
                false,
            )
            .await?;
            let mut running = Running::start(&path, &reference.url, "synthetic-api").await?;
            let mut peer = connect(&path, "law").await?;
            let first = result("before", "\"\"", "lifecycle^ANALYZER", "1.0", before);
            assert!(
                hl7(&mut peer, first.as_bytes())
                    .await?
                    .contains("MSA|AA|before")
            );
            if before == "R" && after == "F" {
                running.stop().await?;
                running = Running::start(&path, &reference.url, "synthetic-api").await?;
                peer = connect(&path, "law").await?;
            }
            let permitted = matches!(
                (before, after),
                ("P", "P" | "R" | "F") | ("R", "R" | "F") | ("F", "C") | ("C", "C")
            );
            let second = result("after", "\"\"", "lifecycle^ANALYZER", "2.0", after);
            let ack = hl7(&mut peer, second.as_bytes()).await?;
            assert!(
                ack.contains(&format!(
                    "MSA|{}|after",
                    if permitted { "AA" } else { "AR" }
                )),
                "{before}->{after}: {ack}"
            );
            assert_eq!(
                count(&path, "observation_revisions")?,
                if permitted { 2 } else { 1 },
                "{before}->{after}"
            );
            let deliveries = i64::from(before != "R") + i64::from(permitted && after != "R");
            assert_eq!(count(&path, "outbox")?, deliveries, "{before}->{after}");
            if permitted && before == "R" && after == "F" {
                let row =
                    database(&path)?.query_row("SELECT payload FROM outbox LIMIT 1", [], |r| {
                        r.get::<_, Vec<u8>>(0)
                    })?;
                let body: serde_json::Value = serde_json::from_slice(&row)?;
                assert_eq!(
                    body.pointer("/results/0/identity/resolved/revision")
                        .and_then(serde_json::Value::as_u64),
                    Some(1)
                );
                assert_eq!(
                    body.pointer("/results/0/parts/0/source_status")
                        .and_then(|v| v.as_str()),
                    Some("F")
                );
            }
            running.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}

// C.7 Figure C.7-1 p.111 and OG.1 pp.102-103; ADR 0011.
#[tokio::test]
async fn law_adr0011_c7_only_one_final_run_in_history_or_batch() -> Result<(), Error> {
    for batch in [false, true] {
        let temp = tempfile::tempdir()?;
        let path = vendor_config(temp.path(), "law", &definition())?;
        let mut reference = Reference::start(
            &temp.path().join("receiver.db"),
            TcpListener::bind("127.0.0.1:0").await?,
            false,
        )
        .await?;
        let running = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "law").await?;
        let first = result("run-one", "\"\"", "order^ANALYZER", "1.0", "F")
            .replace("^99LAB|1|1.0", "^99LAB|1^1^1|1.0");
        let second = result("run-two", "\"\"", "order^ANALYZER", "2.0", "F")
            .replace("^99LAB|1|2.0", "^99LAB|2^1^1|2.0");
        if batch {
            let (_, body) = second.split_once('\r').ok_or("header")?;
            let mixed = format!("{first}{}", body.replacen("SPM|1|", "SPM|2|", 1));
            assert!(
                hl7(&mut peer, mixed.as_bytes())
                    .await?
                    .contains("MSA|AR|run-one")
            );
            assert_eq!(count(&path, "outbox")?, 0);
            assert_eq!(count(&path, "observation_revisions")?, 0);
        } else {
            assert!(
                hl7(&mut peer, first.as_bytes())
                    .await?
                    .contains("MSA|AA|run-one")
            );
            reference.event().await?;
            running.stop().await?;
            let running = Running::start(&path, &reference.url, "synthetic-api").await?;
            let mut peer = connect(&path, "law").await?;
            assert!(
                hl7(&mut peer, second.as_bytes())
                    .await?
                    .contains("MSA|AR|run-two")
            );
            assert!(
                hl7(&mut peer, second.replace("|F|", "|C|").as_bytes())
                    .await?
                    .contains("MSA|AR|run-two")
            );
            assert_eq!(count(&path, "outbox")?, 1);
            assert_eq!(count(&path, "observation_revisions")?, 1);
            running.stop().await?;
            reference.close().await?;
            continue;
        }
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
