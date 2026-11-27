//! De-identifies a real record-only capture through the audited command and replays the copy (ADR 0034, ADR 0037).
use super::{Error, Recorder};
use analyte::operator::captures::deidentify::{MARK, checklist};
use analyte_transport::recording::{self, Direction, Evidence, Exception};
use std::time::Duration;

/// Invented "patient" values; none may be left in the copy.
const VALUES: &[&str] = &[
    "PATX7781",
    "ZORBALIUS",
    "QUENTINA",
    "19710315",
    "SMPZ9981",
    "ZEPHYRINE",
];

async fn exchange(run: &mut Recorder, message: &[u8]) -> Result<(), Error> {
    let wire = analyte_framing::hl7::encode(message, analyte_framing::hl7::Limit::new(1048576)?)?;
    run.send(&wire).await?;
    run.until(b"\x1c\r").await?;
    Ok(())
}

fn command(path: &std::path::Path, arguments: &[&str]) -> Result<std::process::Output, Error> {
    Ok(std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("--config")
        .arg(path)
        .args(["--actor", "it-test", "captures", "de-identify", "recorder"])
        .args(arguments)
        .output()?)
}

/// The host's MSH-7 time and MSH-10 control ID change on every run; they are
/// named, fixed byte locations in each host event (ADR 0009).
fn generated(events: &[recording::Event]) -> Vec<Exception> {
    let mut found = Vec::new();
    for (event, item) in events.iter().enumerate() {
        if item.direction != Direction::HostToMachine || item.link.is_some() {
            continue;
        }
        let mut offset = 0_usize;
        for (field, value) in item.bytes.split(|b| *b == b'|').take(10).enumerate() {
            let end = offset.saturating_add(value.len());
            if field == 6 || field == 9 {
                found.push(Exception {
                    name: format!("generated MSH-{}", field.saturating_add(1)),
                    event,
                    bytes: offset..end,
                });
            }
            offset = end.saturating_add(1);
        }
    }
    found
}

// ADR 0037 with HL7 2.5.1 ch.3 PID, ch.7 OBR, ch.2 NTE and 2.3.1 QRD; ADR 0034
// record-only replies; AGENTS.md safety rule 3: the original is never changed.
#[tokio::test]
async fn captured_conversation_de_identifies_through_the_command_and_replays() -> Result<(), Error>
{
    let mut run = Recorder::start("hl7", "").await?;
    exchange(
        &mut run,
        b"MSH|^~\\&|ANALYZER|LAB|LIS|LAB|20261123120000||ORU^R01^ORU_R01|result1|P|2.5.1\rPID|1||PATX7781||ZORBALIUS^QUENTINA||19710315|F\rOBR|1|000SMPZ9981\rNTE|1||patient says ZEPHYRINE\rOBX|1|NM|SYN||5.2|mg/L|||||F\r",
    )
    .await?;
    exchange(
        &mut run,
        b"MSH|^~\\&|ANALYZER|LAB|LIS|LAB|20261123120100||QRY^Q02|query1|P|2.3.1\rQRD|20261123120100|R|D|query1|||RD|000SMPZ9981|OTH|||T\rQRF|ANALYZER|||||RCT|COR|ALL\r",
    )
    .await?;
    let finished = run
        .finish(["tcp_server", "mllp", "hl7_receiver", "hl7_v2"], 0)
        .await?;
    let folder = finished.directory.path().join("data/captures/recorder");
    let file = std::fs::read_dir(&folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|path| path.to_string_lossy().ends_with(".rec.jsonl"))
        .ok_or("capture missing")?;
    let name = file
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("capture name")?
        .to_owned();
    let original = std::fs::read(&file)?;
    let out = tempfile::tempdir()?;
    let to = out.path().to_str().ok_or("folder")?;
    let output = command(
        &finished.path,
        &[&name, "--to", to, "--reason", "Share with the project"],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let said = String::from_utf8(output.stdout)?;
    assert!(said.contains(checklist()), "{said}");
    assert!(said.contains("contain patient data"), "{said}");
    for value in VALUES {
        assert!(!said.contains(value), "the command never prints values");
    }
    assert_eq!(
        std::fs::read(&file)?,
        original,
        "the original is never changed"
    );
    let stem = name.strip_suffix(".rec.jsonl").ok_or("suffix")?;
    let copy_path = out.path().join(format!("{stem}.deidentified.rec.jsonl"));
    let copied = std::fs::read(&copy_path)?;
    let (header, events) = recording::read(&copied[..], recording::Limits::default())?;
    let (_, before) = recording::read(&original[..], recording::Limits::default())?;
    let joined = |events: &[recording::Event]| -> Vec<u8> {
        events
            .iter()
            .flat_map(|event| event.bytes.clone())
            .collect()
    };
    for value in VALUES {
        let found = |bytes: &[u8]| bytes.windows(value.len()).any(|w| w == value.as_bytes());
        assert!(found(&joined(&before)), "{value} was captured");
        assert!(!found(&joined(&events)), "{value} survived");
    }
    assert!(header.de_identification.starts_with(MARK));
    assert_eq!(header.evidence, Evidence::Machine);
    assert_eq!(events.len(), before.len());
    // The audit keeps who did it and why.
    let db = rusqlite::Connection::open(finished.directory.path().join("data/analyte.db"))?;
    let audited: i64 = db.query_row(
        "SELECT count(*) FROM audit WHERE evidence LIKE '%captures-de-identify%' AND evidence LIKE '%Share with the project%'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(audited, 1);
    // A second run never overwrites the copy.
    let again = command(&finished.path, &[&name, "--to", to, "--reason", "Again"])?;
    assert!(!again.status.success());
    let refusal = String::from_utf8_lossy(&again.stderr);
    assert!(refusal.contains("Exists"), "{refusal}");
    assert_eq!(std::fs::read(&copy_path)?, copied);
    // The copy replays through a fresh record-only machine with the same replies.
    let mut replay = Recorder::start("hl7", "").await?;
    analyte_sim::replay(
        &mut replay.peer,
        &events,
        &generated(&events),
        Duration::from_secs(10),
    )
    .await?;
    replay.stop().await?;
    Ok(())
}
