//! Tests packed ASTM records on a record-only connection: strict by default, packed only for the cobas e 411's cobas protocol type (ADR 0015, ADR 0034).
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
