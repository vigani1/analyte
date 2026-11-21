//! Tests POCT identity ambiguity through custody, reconciliation and the receiver.
use super::*;

// POCT01-A2 Appendix B 6.10 pp.121-123, 5.21 p.105; ADR 0003 clinical rule 4.
// CLSI POCT01-A2 §B.6.10
#[tokio::test]
async fn repeated_analyte_without_identity_is_parked() -> Result<(), Error> {
    let definition = include_str!("../../../core/config/examples/poct.toml").rewrite(
        "[poct]",
        "[poct]\nidentity = { sequence_number = true, processing = \"Production\" }",
    );
    for reason in ["NEW", "RES", "EDT"] {
        for second_value in ["12.30", "99.30"] {
            let mut run = Running::start(&definition).await?;
            poct_start(&mut run, &poct_peer::hello("hello")).await?;
            let first =
                poct_peer::observation("siblings", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
            let start = first.find("<OBS>").ok_or("missing OBS start")?;
            let end = first
                .find("</OBS>")
                .ok_or("missing OBS end")?
                .checked_add(6)
                .ok_or("OBS end overflow")?;
            let second = first
                .get(start..end)
                .ok_or("OBS bounds")?
                .rewrite("12.30", second_value);
            let raw = first.rewrite("</PT>", &format!("{second}</PT><SPC><SPC.specimen_id V=\"000xyz\"/><SPC.specimen_dttm NULL=\"UNK\"/><SPC.source_cd NULL=\"UNK\"/><SPC.type_cd NULL=\"UNK\"/></SPC>"))
                .rewrite("<SVC.sequence_nbr", &format!("<SVC.reason_cd V=\"{reason}\"/><SVC.sequence_nbr"));
            run.peer.write_all(raw.as_bytes()).await?;
            let ack = xml(&mut run.peer).await?;
            assert!(
                matches!(analyte_poct::Message::parse(&ack, poct_peer::limits())?.body(), analyte_poct::Body::Acknowledgement { kind: analyte_poct::AckKind::Accept, reference } if reference.as_str() == "siblings")
            );
            run.stored(raw.as_bytes(), "analyte-poct")?;
            let db = run.database()?;
            let (receipt, encoded): (String, String) = db.query_row(
                "SELECT r.id,i.evidence FROM receipts r JOIN interpretations i ON i.receipt=r.id WHERE r.raw=?1", [raw.as_bytes()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let reading: analyte_store::Interpretation = serde_json::from_str(&encoded)?;
            let analyte_store::Reading::Results(results) = reading.outcome else {
                return Err("expected mapped siblings".into());
            };
            assert_eq!(results.len(), 2);
            for result in &results {
                assert!(
                    matches!(&result.identity, analyte_model::IdentityStatus::Unresolved(reason) if reason.as_str() == "POCT repeated derived observation identity without a distinguishing sub-ID")
                );
            }
            assert_eq!(db.query_row(
                "SELECT count(*) FROM parked p WHERE json_extract(p.evidence,'$.kind')='mapping_finding' AND json_extract(p.evidence,'$.reason')='POCT repeated derived observation identity without a distinguishing sub-ID' AND EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract(?1,'$'))",
                [&receipt], |r| r.get::<_, i64>(0))?, 1);
            let mut parked = db.prepare("SELECT json_extract(p.evidence,'$.context.Result.index'), json_extract(p.evidence,'$.reason') FROM parked p WHERE json_extract(p.evidence,'$.kind')='result_reconciliation' AND EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract(?1,'$')) ORDER BY p.id")?;
            let items: Vec<(i64, String)> = parked
                .query_map([receipt], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            // Both results share one cause, named once (ADR 0024); the
            // results keep their own reason in the stored reading above.
            assert_eq!(items, Vec::new());
            assert_eq!(
                db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                0
            );
            run.close().await?;
        }
    }
    Ok(())
}
