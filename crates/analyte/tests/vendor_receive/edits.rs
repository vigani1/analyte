//! Tests conservative POCT edits through durable reconciliation and the receiver.
use super::*;
const EDIT_REASON: &str = "edited POCT observation with no detectable common-content change; source context may have changed";

// POCT01-A2 B 3.2, 5.15, 5.18, 5.21 Table 49 and 5.22; project decision 2026-11-16.
#[tokio::test]
async fn poct01_b_5_21_table49_patient_and_metadata_edits_are_not_retransmissions()
-> Result<(), Error> {
    let definition = include_str!("../../../core/config/examples/poct.toml").replace(
        "[poct]",
        "[poct]\nidentity = { sequence_number = true, processing = \"Production\" }",
    );
    let mut run = Running::start(&definition).await?;
    poct_start(&mut run, &poct_peer::hello("hello")).await?;
    let original = poct_peer::observation("original", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>")
        .replace("</OBS>", "<OBS.normal_lo-hi_limit V=\"[1;20]\" U=\"mg/L\"/><OBS.critical_lo-hi_limit V=\"[0;30]\" U=\"mg/L\"/></OBS>")
        .replace("</PT>", "</PT><SPC><SPC.specimen_id V=\"000xyz\" SN=\"local\"/><SPC.specimen_dttm NULL=\"UNK\"/><SPC.source_cd NULL=\"UNK\"/><SPC.type_cd NULL=\"UNK\"/></SPC>");
    run.peer.write_all(original.as_bytes()).await?;
    xml(&mut run.peer).await?;
    poct_deliveries(&run, 1).await?;
    for (i, changed) in [
        original.replace(
            "PT.patient_id V=\"000xyz\"",
            "PT.patient_id V=\"other-patient\"",
        ),
        original.replace("OBS.method_cd V=\"M\"", "OBS.method_cd V=\"C\""),
        original.replace("[0;30]", "[0;40]"),
        original.replace("[1;20]\" U=\"mg/L", "[1;20]\" U=\"g/L"),
        original.replace(
            "SN=\"synthetic-system\"",
            "SN=\"synthetic-system\" SV=\"changed\"",
        ),
        original.replace("SN=\"local\"", "SN=\"other-local\""),
    ]
    .into_iter()
    .enumerate()
    {
        let raw = changed
            .replace("V=\"original\"", &format!("V=\"edit-{i}\""))
            .replace(
                "<SVC.sequence_nbr",
                "<SVC.reason_cd V=\"EDT\"/><SVC.sequence_nbr",
            );
        run.peer.write_all(raw.as_bytes()).await?;
        xml(&mut run.peer).await?;
        assert_parked_edit(&run, &raw, 1)?;
    }
    for reason in ["NEW", "RES"] {
        let raw = original
            .replace("V=\"original\"", &format!("V=\"repeat-{reason}\""))
            .replace(
                "<SVC.sequence_nbr",
                &format!("<SVC.reason_cd V=\"{reason}\"/><SVC.sequence_nbr"),
            );
        run.peer.write_all(raw.as_bytes()).await?;
        xml(&mut run.peer).await?;
        let count: i64 = run
            .database()?
            .query_row("SELECT count(*) FROM parked", [], |r| r.get(0))?;
        assert_eq!(count, 6);
    }
    // The changed-value EDT is delivered once. Its ACK is deliberately unread.
    let edited = original
        .replace("12.30", "13.30")
        .replace("V=\"original\"", "V=\"value-edit\"")
        .replace(
            "<SVC.sequence_nbr",
            "<SVC.reason_cd V=\"EDT\"/><SVC.sequence_nbr",
        );
    run.peer.write_all(edited.as_bytes()).await?;
    poct_deliveries(&run, 2).await?;
    for _ in 0..2 {
        // The peer reconnects without ever reading the original EDT ACK and resends exact bytes.
        run.peer = TcpStream::connect(run.listener.local_addr()?).await?;
        poct_start(&mut run, &poct_peer::hello("hello")).await?;
        run.peer.write_all(edited.as_bytes()).await?;
        xml(&mut run.peer).await?;
        assert_parked_edit(&run, &edited, 2)?;
    }
    assert_eq!(
        run.database()?.query_row(
            "SELECT count(*) FROM parked WHERE json_extract(evidence,'$.reason')=?1",
            [EDIT_REASON],
            |r| r.get::<_, i64>(0)
        )?,
        8
    );
    assert_eq!(
        run.database()?.query_row(
            "SELECT count(*) FROM receipts r JOIN interpretations i ON i.receipt=r.id WHERE r.raw=?1",
            [edited.as_bytes()],
            |r| r.get::<_, i64>(0)
        )?,
        3
    );
    run.close_with_deliveries(2).await
}

fn assert_parked_edit(run: &Running, raw: &str, revisions: i64) -> Result<(), Error> {
    run.stored(raw.as_bytes(), "analyte-poct")?;
    let db = run.database()?;
    let count: i64 = db.query_row("SELECT count(*) FROM parked p JOIN receipts r ON EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract(r.id,'$')) WHERE r.rowid=(SELECT max(rowid) FROM receipts WHERE raw=?1) AND json_extract(p.evidence,'$.reason')=?2", rusqlite::params![raw.as_bytes(), EDIT_REASON], |r| r.get(0))?;
    assert_eq!(count, 1);
    assert_eq!(db.query_row("SELECT count(*) FROM parked p WHERE EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract((SELECT id FROM receipts WHERE raw=?1 ORDER BY rowid DESC LIMIT 1),'$'))", [raw.as_bytes()], |r| r.get::<_, i64>(0))?, 1);
    assert_eq!(db.query_row("SELECT count(*) FROM outbox WHERE receipt=(SELECT id FROM receipts WHERE raw=?1 ORDER BY rowid DESC LIMIT 1)", [raw.as_bytes()], |r| r.get::<_, i64>(0))?, 0);

    assert_eq!(
        db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        revisions
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
            .get::<_, i64>(0))?,
        revisions
    );
    Ok(())
}
