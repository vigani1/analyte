//! Tests packed ASTM records on a record-only connection: strict by default, packed only for the cobas e 411's cobas protocol types, from its record file or its ready definition (ADR 0015, ADR 0034).
use super::{Error, Recorder};
use analyte_framing::astm::{FrameNumber, Limit};

const ACK: u8 = 6;
const NAK: u8 = 21;
const ENQ: u8 = 5;
const EOT: u8 = 4;

/// One complete result message packed into a single frame, as the cobas
/// protocol type may send it.
const PACKED_RESULT: &[u8] = b"H|\\^&|||Analyzer|||||||P|LIS2-A2|20261123120000\rP|1\rO|1|000123||^^^SYN|R\rR|1|^^^SYN|5.2|mg/L||N||F\rL|1|N\r";

/// The stack a record-only ASTM capture names.
const STACK: [&str; 4] = ["tcp_server", "astm_lis01", "astm_link", "astm_lis02"];

/// The cobas e 411 record-only test fixture with a loopback listener in place of
/// its serial link; everything else, the departure above all, as shipped.
fn cobas_on_loopback() -> Result<String, Error> {
    let mut table: toml::Table = analyte_fixtures::ASTM_PACKED_RECORD_ONLY.parse()?;
    let mut transport = toml::Table::new();
    transport.insert("kind".into(), "tcp_server".into());
    table.insert("transport".into(), transport.into());
    Ok(toml::to_string(&table)?)
}

async fn cobas(protocol_type: &str) -> Result<Recorder, Error> {
    Recorder::start_model(
        "roche/cobas-e411",
        &cobas_on_loopback()?,
        &format!("analyzer_mode = \"{protocol_type}\"\n"),
    )
    .await
}

/// `text` as one frame numbered 1, its checksum and LIS01 envelope intact.
fn one_frame(text: &[u8]) -> Result<Vec<u8>, Error> {
    Ok(
        analyte_framing::astm::encode(text, FrameNumber::FIRST, Limit::new(64000)?)?
            .next()
            .ok_or("frame")?,
    )
}

/// Opens a transfer, sends `frame` and returns the reply to it.
async fn offer(run: &mut Recorder, frame: &[u8]) -> Result<u8, Error> {
    run.send(&[ENQ]).await?;
    assert_eq!(run.byte().await?, ACK, "ENQ accepted");
    run.send(frame).await?;
    run.byte().await
}

// CLSI LIS01-A2 3.3.2, 6.3.1.1 and 8.3.1.1 (a message, which is one record,
// never shares a frame) and
// ADR 0015 (nothing departs by default): the shared ASTM capture profile and
// the cobas e 411 in its Elecsys protocol type (HIM v2.3 section 5, p. 14;
// v2.2 D section 1) refuse a packed frame with NAK, and keep its bytes.
#[tokio::test]
async fn strict_record_only_refuses_a_packed_frame_and_keeps_it() -> Result<(), Error> {
    for mut run in [Recorder::start("astm", "").await?, cobas("elecsys").await?] {
        let reply = offer(&mut run, &one_frame(PACKED_RESULT)?).await?;
        assert_eq!(reply, NAK, "one record per frame unless declared");
        run.send(&[EOT]).await?;
        run.finish(STACK, 0).await?;
    }
    Ok(())
}

// cobas e 411 HIM v2.3 section 5 (p. 14) and v2.2 D section 1 (p. D-2): in
// the cobas protocol type a frame may hold several records; ADR 0015 packing
// is declared for that type only, and LIS01-A2 6.3.1 still checks the frame.
// LIS02-A2 12.3: a packed query is still answered with code Q, never "no
// information" (rule 9).
#[tokio::test]
async fn cobas_protocol_type_acknowledges_a_packed_frame_and_keeps_it() -> Result<(), Error> {
    let mut run = cobas("cobas").await?;
    let reply = offer(&mut run, &one_frame(PACKED_RESULT)?).await?;
    assert_eq!(reply, ACK, "a verified packed frame is acknowledged");
    run.send(&[EOT]).await?;
    run.kept_everything().await?;
    let query = one_frame(
        b"H|\\^&|||Analyzer|||||||P|LIS2-A2|20261123120100\rQ|1|^000124||ALL||||||||O\rL|1|N\r",
    )?;
    assert_eq!(offer(&mut run, &query).await?, ACK);
    run.send(&[EOT]).await?;
    assert_eq!(run.byte().await?, ENQ, "the host opens its answer");
    run.send(&[ACK]).await?;
    let mut answer = Vec::new();
    loop {
        let first = run.byte().await?;
        if first == EOT {
            break;
        }
        answer.push(first);
        answer.extend(run.until(b"\n").await?);
        run.send(&[ACK]).await?;
    }
    let answer = String::from_utf8(answer)?;
    assert!(answer.contains("L|1|Q\r"), "{answer:?}");
    assert!(
        !answer.contains("L|1|I"),
        "never no information: {answer:?}"
    );
    run.finish(STACK, 0).await?;
    Ok(())
}

// CLSI LIS01-A2 6.5.1, 6.5.1.1 and 6.5.1.2 (a frame failing its checksum is
// refused and sent again with the same number) with ADR 0015 (a departure never loosens the checksum): in
// the cobas protocol type a packed frame with a wrong checksum gets NAK and
// is kept; its corrected retry is acknowledged.
#[tokio::test]
async fn cobas_protocol_type_still_refuses_a_bad_checksum() -> Result<(), Error> {
    let mut run = cobas("cobas").await?;
    let good = one_frame(PACKED_RESULT)?;
    let mut bad = good.clone();
    let checksum = bad.len().checked_sub(4).ok_or("frame length")?;
    let digit = bad.get_mut(checksum).ok_or("checksum")?;
    *digit = if *digit == b'0' { b'1' } else { b'0' };
    assert_eq!(offer(&mut run, &bad).await?, NAK, "bad checksum");
    run.send(&good).await?;
    assert_eq!(run.byte().await?, ACK, "the retry is acknowledged");
    run.send(&[EOT]).await?;
    run.finish(STACK, 0).await?;
    Ok(())
}

/// The shipped cobas e 411 definition with a loopback listener in place of
/// its serial link; its choices and departures as shipped.
fn ready_cobas_on_loopback() -> Result<String, Error> {
    let mut table: toml::Table =
        include_str!("../../../../machines/roche/cobas-e411/machine.toml").parse()?;
    let mut transport = toml::Table::new();
    transport.insert("kind".into(), "tcp_server".into());
    transport.insert("operation_timeout_ms".into(), 15000.into());
    table.insert("transport".into(), transport.into());
    Ok(toml::to_string(&table)?)
}

async fn ready_cobas(protocol_type: &str) -> Result<Recorder, Error> {
    Recorder::start_file(
        "roche/cobas-e411",
        "machine.toml",
        &ready_cobas_on_loopback()?,
        &format!("analyzer_mode = \"{protocol_type}\"\n"),
    )
    .await
}

/// Every byte sent so far is already stored when this runs: called right
/// after a reply, it proves the store came first (AGENTS.md clinical rule 1).
fn stored_before_reply(run: &Recorder) -> Result<(), Error> {
    assert_eq!(run.kept()?, run.sent, "stored before the reply");
    Ok(())
}

// cobas e 411 HIM v2.3 section 5 (p. 14) and v2.2 D section 1 (p. D-2), with
// ADR 0015 and ADR 0034 (amendment of 2026-11-27): a lab recording a ready
// cobas e 411 in a cobas protocol type gets a verified packed frame ACKed,
// each byte stored before the reply; ADR 0010 sets the frame as the boundary.
#[tokio::test]
async fn ready_cobas_definition_records_a_packed_frame_after_storing_it() -> Result<(), Error> {
    for protocol_type in ["cobas", "cobas_qc_lot"] {
        let mut run = ready_cobas(protocol_type).await?;
        let reply = offer(&mut run, &one_frame(PACKED_RESULT)?).await?;
        assert_eq!(reply, ACK, "{protocol_type}: a verified packed frame");
        stored_before_reply(&run)?;
        run.send(&[EOT]).await?;
        run.finish(STACK, 0).await?;
    }
    Ok(())
}

// CLSI LIS01-A2 6.5.1 and 6.5.1.1 (a frame failing its checksum is refused)
// with ADR 0015 (a departure never loosens the checksum) and ADR 0034
// decision 10: through the ready cobas definition a broken packed frame is
// NAKed and still kept, stored before the NAK.
#[tokio::test]
async fn ready_cobas_definition_refuses_a_broken_packed_frame_and_keeps_it() -> Result<(), Error> {
    let mut run = ready_cobas("cobas").await?;
    let mut bad = one_frame(PACKED_RESULT)?;
    let checksum = bad.len().checked_sub(4).ok_or("frame length")?;
    let digit = bad.get_mut(checksum).ok_or("checksum")?;
    *digit = if *digit == b'0' { b'1' } else { b'0' };
    assert_eq!(offer(&mut run, &bad).await?, NAK, "bad checksum");
    stored_before_reply(&run)?;
    run.send(&[EOT]).await?;
    run.finish(STACK, 0).await?;
    Ok(())
}

// CLSI LIS01-A2 6.3.1.1 (one record per frame) with cobas e 411 HIM v2.3
// section 5 (p. 14): the ready definition declares packing for its cobas
// protocol types only, so its Elecsys type refuses a packed frame with NAK
// and keeps it.
#[tokio::test]
async fn ready_cobas_definition_in_elecsys_mode_refuses_a_packed_frame() -> Result<(), Error> {
    let mut run = ready_cobas("elecsys").await?;
    assert_eq!(offer(&mut run, &one_frame(PACKED_RESULT)?).await?, NAK);
    stored_before_reply(&run)?;
    run.send(&[EOT]).await?;
    run.finish(STACK, 0).await?;
    Ok(())
}
