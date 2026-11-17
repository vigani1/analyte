//! Tests LAW read custody on session interruption.
use super::*;

// ADR 0003 rules 1–2: cancellation during a merged read keeps both custody regions.
#[tokio::test]
async fn adr0003_law_mid_read_parks_input_prefix_and_tail() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let mut loaded = program(&path, &reference.url)?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    loaded.pause_law_committed_for_test("law", hook)?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let wire = analyte_framing::hl7::encode(
        result("mid-read", "\"\"", "awos-1^LAB", "12.30", "F").as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?;
    let tail = b"tail";
    let mut merged = wire.clone();
    merged.extend_from_slice(tail);
    peer.write_all(&merged).await?;
    control.reached().await?;
    running.stop().await?;
    for (raw, reason) in [
        (
            wire.as_slice(),
            "Interrupted input: fed prefix; possible duplicate of dialog receipts",
        ),
        (
            tail.as_slice(),
            "Interrupted input: untouched transport tail",
        ),
    ] {
        let retained: i64 = database(&path)?.query_row(
            "SELECT count(*) FROM receipts r JOIN parked p ON json_extract(r.id,'$')=json_extract(p.evidence,'$.targets[0]') WHERE r.raw=?1 AND json_extract(p.evidence,'$.reason')=?2",
            rusqlite::params![raw, reason], |row| row.get(0),
        )?;
        assert_eq!(retained, 1, "{reason}");
    }
    reference.close().await?;
    Ok(())
}
