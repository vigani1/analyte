//! Checks de-identified copies of synthetic recordings for every supported protocol.
//!
//! Every value here is invented. The distinctive "patient" values must not
//! survive anywhere in the copy, while results, units, times and every frame
//! stay valid and unchanged.
use super::*;
use analyte_framing::{astm, autolyser as envelope, hl7 as mllp, vidas};
use analyte_transport::recording::{Direction, Evidence, Link};
use proptest::prelude::*;

type Fallible = Result<(), Box<dyn std::error::Error>>;

const MACHINE: Direction = Direction::MachineToHost;
const HOST: Direction = Direction::HostToMachine;

fn header(stack: [&str; 4]) -> Header {
    Header {
        version: 1,
        stack: stack.map(str::to_owned),
        machine: Some("bench-1".into()),
        software: None,
        evidence: Evidence::Machine,
        de_identification: "NOT de-identified".into(),
        source: "Host transport wire tap".into(),
        exceptions: Vec::new(),
    }
}

fn recorded(parts: Vec<(Direction, Vec<u8>)>) -> Vec<Event> {
    let mut events = vec![Event {
        direction: MACHINE,
        time_ms: 0,
        bytes: Vec::new(),
        link: Some(Link::Connect),
    }];
    for (index, (direction, bytes)) in parts.into_iter().enumerate() {
        events.push(Event {
            direction,
            time_ms: u64::try_from(index).unwrap_or(0).saturating_mul(10),
            bytes,
            link: None,
        });
    }
    events
}

/// A machine write split in two reads, as a serial port often delivers it.
fn split(direction: Direction, bytes: &[u8], parts: &mut Vec<(Direction, Vec<u8>)>) {
    let middle = bytes.len() / 2;
    parts.push((direction, bytes.get(..middle).unwrap_or_default().to_vec()));
    parts.push((direction, bytes.get(middle..).unwrap_or_default().to_vec()));
}

fn lane(events: &[Event], direction: Direction) -> Vec<u8> {
    events
        .iter()
        .filter(|event| event.link.is_none() && event.direction == direction)
        .flat_map(|event| event.bytes.iter().copied())
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Asserts the copy keeps every event's direction, time, link and length,
/// and that none of `values` is left anywhere in it.
fn assert_clean(original: &[Event], copy: &Copy, values: &[&str]) {
    assert_eq!(copy.events.len(), original.len());
    for (was, now) in original.iter().zip(&copy.events) {
        assert_eq!(was.direction, now.direction);
        assert_eq!(was.time_ms, now.time_ms);
        assert_eq!(was.link, now.link);
        assert_eq!(was.bytes.len(), now.bytes.len());
    }
    let all: Vec<u8> = copy.events.iter().flat_map(|e| e.bytes.clone()).collect();
    let before: Vec<u8> = original.iter().flat_map(|e| e.bytes.clone()).collect();
    for value in values {
        assert!(
            contains(&before, value.as_bytes()),
            "{value} is in the original"
        );
        assert!(!contains(&all, value.as_bytes()), "{value} survived");
    }
    assert!(copy.header.de_identification.starts_with(MARK));
    assert_eq!(copy.header.evidence, Evidence::Machine);
}

const ASTM_STACK: [&str; 4] = ["serial", "astm_lis01", "astm_link", "astm_lis02"];
const ASTM_VALUES: &[&str] = &[
    "PATX7781",
    "LABQ5512",
    "ZORBALIUS",
    "QUENTINA",
    "WISTERIA",
    "5550199",
    "19710315",
    "ZEPHYRINE",
    "KALVORN",
    "SMPZ9981",
];

/// LIS01-A2 frames of `records`, one record per frame, split into frames of
/// at most `limit` bytes, with the frame number carried across records.
fn frames(records: &[&[u8]], limit: usize) -> Result<Vec<Vec<u8>>, astm::Error> {
    let mut number = astm::FrameNumber::FIRST;
    let mut all = Vec::new();
    for record in records {
        for frame in astm::encode(record, number, astm::Limit::new(limit)?)? {
            number = number.next();
            all.push(frame);
        }
    }
    Ok(all)
}

fn astm_upload(id: &str, name: &str) -> Result<Vec<Event>, astm::Error> {
    let patient = format!(
        "P|1|{id}|LABQ5512||ZORBALIUS^{name}||19710315|F||77 WISTERIA LANE||5550199|KALVORN\r"
    );
    let upload = frames(
        &[
            b"H|\\^&|||Bench^1|||||host||P|1\r",
            patient.as_bytes(),
            b"C|1|L|patient says ZEPHYRINE|G\r",
            b"O|1|000SMPZ9981|0083^@19^04|^^^874|R||20260101080000\r",
            b"R|1|^^^874|8.64|ng/mL|1-10|N||F||OPER|20260101080000|20260101090000|E1\r",
            b"C|1|I|instrument flag text|I\r",
            b"L|1|N\r",
        ],
        40,
    )?;
    let reply = frames(
        &[
            b"H|\\^&|||Analyte^1|||||Bench||P|1\r",
            format!("P|1|{id}\r").as_bytes(),
            b"O|1|000SMPZ9981||^^^874|R\r",
            b"L|1|N\r",
        ],
        64000,
    )?;
    let mut parts = vec![(MACHINE, vec![astm::ENQ]), (HOST, vec![astm::ACK])];
    for (index, frame) in upload.iter().enumerate() {
        if index == 1 {
            // The P frame is refused once and sent again (LIS01-A2 §8.5.1).
            parts.push((MACHINE, frame.clone()));
            parts.push((HOST, vec![astm::NAK]));
        }
        if index == 3 {
            // A frame whose checksum was corrupted on the line, then its good repeat.
            let mut bad = frame.clone();
            let at = bad.len().saturating_sub(3);
            if let Some(digit) = bad.get_mut(at) {
                *digit = if *digit == b'0' { b'1' } else { b'0' };
            }
            parts.push((MACHINE, bad));
            parts.push((HOST, vec![astm::NAK]));
        }
        split(MACHINE, frame, &mut parts);
        parts.push((HOST, vec![astm::ACK]));
    }
    parts.push((MACHINE, vec![astm::EOT]));
    parts.push((HOST, vec![astm::ENQ]));
    parts.push((MACHINE, vec![astm::ACK]));
    for frame in reply {
        parts.push((HOST, frame));
        parts.push((MACHINE, vec![astm::ACK]));
    }
    parts.push((HOST, vec![astm::EOT]));
    Ok(recorded(parts))
}

/// Accepted frame texts of one lane, and how many frames were rejected.
fn astm_text(bytes: &[u8]) -> (Vec<u8>, usize) {
    let mut decoder = astm::Decoder::new(astm::Limit::default(), astm::HexPolicy::Strict);
    let mut text = Vec::new();
    let mut rejected = 0_usize;
    let mut last: Option<Vec<u8>> = None;
    for byte in bytes {
        match decoder.push(*byte) {
            Some(astm::Event::Frame(frame)) => {
                // A repeated frame is the same frame again, not new text.
                if last.as_deref() != Some(frame.raw()) {
                    text.extend_from_slice(frame.text());
                }
                last = Some(frame.raw().to_vec());
            }
            Some(astm::Event::Rejected { .. }) => rejected = rejected.saturating_add(1),
            Some(astm::Event::Outside(_)) | None => (),
        }
    }
    (text, rejected)
}

fn records_of(text: &[u8], kind: u8) -> Vec<Vec<u8>> {
    text.split(|b| *b == b'\r')
        .filter(|record| record.first() == Some(&kind))
        .map(<[u8]>::to_vec)
        .collect()
}

fn field(record: &[u8], index: usize, delimiter: u8) -> Vec<u8> {
    record
        .split(|b| *b == delimiter)
        .nth(index)
        .unwrap_or_default()
        .to_vec()
}

// LIS02-A2 §7 (patient), §8.4 (order), §10 (comment), §9 (result);
// LIS01-A2 §8.3.3 (checksum) and §8.5.1 (repeat after NAK); ADR 0037.
#[test]
fn astm_copy_replaces_patient_fields_and_keeps_results() -> Fallible {
    let events = astm_upload("PATX7781", "QUENTINA")?;
    let copy = de_identify(&header(ASTM_STACK), &events, None)?;
    assert_clean(&events, &copy, ASTM_VALUES);
    let (before, before_rejected) = astm_text(&lane(&events, MACHINE));
    let (after, after_rejected) = astm_text(&lane(&copy.events, MACHINE));
    // Every good frame still checks; the corrupted one is still corrupted.
    assert_eq!((before_rejected, after_rejected), (1, 1));
    assert_eq!(records_of(&before, b'R'), records_of(&after, b'R'));
    assert_eq!(records_of(&before, b'H'), records_of(&after, b'H'));
    let comments = records_of(&after, b'C');
    assert_eq!(comments.get(1), records_of(&before, b'C').get(1));
    let patient = records_of(&after, b'P').concat();
    let birth = field(&patient, 7, b'|');
    assert_eq!(birth.len(), 8);
    assert!(birth.iter().all(u8::is_ascii_digit));
    assert_eq!(field(&patient, 8, b'|'), b"U");
    let order = records_of(&after, b'O').concat();
    let sample = field(&order, 2, b'|');
    assert_eq!(sample.get(..3), Some(b"000".as_slice()));
    // The instrument's rack and position stay; its sequence number is the ID part.
    assert!(field(&order, 3, b'|').ends_with(b"^@19^04"));
    assert_eq!(field(&order, 7, b'|'), b"20260101080000");
    // The same sample and patient get the same stand-ins in the host's reply.
    let (reply, reply_rejected) = astm_text(&lane(&copy.events, HOST));
    assert_eq!(reply_rejected, 0);
    let reply_order = records_of(&reply, b'O').concat();
    assert_eq!(field(&reply_order, 2, b'|'), sample);
    assert_eq!(
        field(&records_of(&reply, b'P').concat(), 2, b'|'),
        field(&patient, 2, b'|')
    );
    assert!(copy.distinct > 0 && copy.replaced >= copy.distinct);
    Ok(())
}

fn astm_refused(records: &[&[u8]]) -> Result<Error, Box<dyn std::error::Error>> {
    let mut parts = vec![(MACHINE, vec![astm::ENQ])];
    for frame in frames(records, 64000)? {
        parts.push((MACHINE, frame));
    }
    match de_identify(&header(ASTM_STACK), &recorded(parts), None) {
        Ok(_) => Err("a copy was made".into()),
        Err(error) => Ok(error),
    }
}

// LIS02-A2 §6 (header first), §14 (manufacturer record); ADR 0037: never half-cleaned.
#[test]
fn astm_records_without_known_fields_refuse_the_file() -> Fallible {
    let before = astm_refused(&[b"P|1|PATX7781\r"])?;
    assert!(
        matches!(before, Error::Unparsed { event: 2, .. }),
        "{before}"
    );
    let manufacturer = astm_refused(&[b"H|\\^&\r", b"M|1|PATX7781\r"])?;
    assert!(
        matches!(manufacturer, Error::Unparsed { .. }),
        "{manufacturer}"
    );
    assert!(!manufacturer.to_string().contains("PATX7781"));
    let unknown = astm_refused(&[b"H|\\^&\r", b"Z|1|PATX7781\r"])?;
    assert!(matches!(unknown, Error::Unparsed { .. }));
    let escaped = astm_refused(&[b"H|\\^&\r", b"P|1|&X4A4F&\r"])?;
    assert!(matches!(escaped, Error::Unparsed { .. }));
    let loose = de_identify(
        &header(ASTM_STACK),
        &recorded(vec![(MACHINE, b"P|1|PATX7781\r".to_vec())]),
        None,
    );
    assert!(matches!(loose, Err(Error::Unparsed { .. })));
    Ok(())
}

const HL7_STACK: [&str; 4] = ["tcp_server", "mllp", "hl7_receiver", "hl7_v2"];
const HL7_VALUES: &[&str] = &[
    "PATX7781",
    "LABQ5512",
    "HOSPQ",
    "ZORBALIUS",
    "QUENTINA",
    "19710315",
    "WISTERIA",
    "5550199",
    "WARD7",
    "KALVORN",
    "SMPZ9981",
    "FILLQ4471",
    "ZEPHYRINE",
];

fn hl7_messages(id: &str, name: &str) -> Vec<Vec<u8>> {
    vec![
        format!(
            "MSH|^~\\&|Bench|Lab|||20260101090000||ORU^R01|m1|P|2.5.1\rPID|1||{id}^^^HOSPQ~LABQ5512||ZORBALIUS^{name}||19710315|F|||77 WISTERIA LANE^^TOWN||5550199\rPV1|1|O|WARD7^BED3||||KALVORN^DR\rORC|RE|000SMPZ9981|FILLQ4471\rOBR|1|000SMPZ9981|FILLQ4471|874^TSH|||20260101080000\rNTE|1||patient says ZEPHYRINE\\.br\\again\rOBX|1|NM|874^TSH||8.64|ng/mL|1-10|N|||F|||20260101090000\rSPM|1|000SMPZ9981&LAB||SER\r"
        )
        .into_bytes(),
        b"MSH|^~\\&|Host|Lab|Bench|Lab|20260101090001||ACK^R01|h1|P|2.5.1\rMSA|AA|m1\r".to_vec(),
        b"MSH|^~\\&|Bench|Lab|||20260101090002||QRY^Q02|m2|P|2.3.1\rQRD|20260101090002|R|D|27|||RD|000SMPZ9981|OTH|||T\rQRF|Bench|||||RCT|COR|ALL\r".to_vec(),
    ]
}

fn hl7_recording(id: &str, name: &str) -> Result<Vec<Event>, mllp::Error> {
    let limit = mllp::Limit::new(65536)?;
    let mut parts = Vec::new();
    for (index, message) in hl7_messages(id, name).iter().enumerate() {
        let wire = mllp::encode(message, limit)?;
        let direction = if index == 1 { HOST } else { MACHINE };
        split(direction, &wire, &mut parts);
    }
    Ok(recorded(parts))
}

fn segment(message: &[u8], name: &[u8]) -> Vec<u8> {
    message
        .split(|b| *b == b'\r')
        .find(|segment| segment.starts_with(name))
        .unwrap_or_default()
        .to_vec()
}

fn blocks(bytes: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let limit = mllp::Limit::new(65536)?;
    let mut found = Vec::new();
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|b| *b == mllp::SB) {
        let tail = rest.get(start..).unwrap_or_default();
        let end = tail
            .windows(2)
            .position(|pair| pair == [mllp::EB, mllp::CR])
            .ok_or("unterminated block")?
            .saturating_add(2);
        found.push(mllp::decode(tail.get(..end).unwrap_or_default(), limit)?.to_vec());
        rest = tail.get(end..).unwrap_or_default();
    }
    Ok(found)
}

// HL7 2.5.1 ch.3 PID/PV1, ch.4 ORC, ch.7 OBR/SPM/OBX, ch.2 NTE and §2.7
// escapes; 2.3.1 ch.2 QRD; MLLP release 1 framing; ADR 0037.
#[test]
fn hl7_copy_replaces_patient_fields_and_keeps_observations() -> Fallible {
    let events = hl7_recording("PATX7781", "QUENTINA")?;
    let copy = de_identify(&header(HL7_STACK), &events, None)?;
    assert_clean(&events, &copy, HL7_VALUES);
    let before = blocks(&lane(&events, MACHINE))?;
    let after = blocks(&lane(&copy.events, MACHINE))?;
    assert_eq!(after.len(), 2);
    let (result, query) = (&after[0], &after[1]);
    for name in [b"MSH".as_slice(), b"OBX"] {
        assert_eq!(segment(result, name), segment(&before[0], name));
    }
    let pid = segment(result, b"PID");
    assert_eq!(field(&pid, 8, b'|'), b"U");
    assert!(field(&pid, 7, b'|').iter().all(u8::is_ascii_digit));
    // The formatting escape in the note stays an escape.
    assert!(contains(&segment(result, b"NTE"), b"\\.br\\"));
    let placer = field(&segment(result, b"OBR"), 2, b'|');
    assert_eq!(field(&segment(query, b"QRD"), 8, b'|'), placer);
    assert_eq!(
        field(&segment(result, b"SPM"), 2, b'|'),
        [placer.as_slice(), b"&LAB"].concat()
    );
    assert_eq!(
        blocks(&lane(&copy.events, HOST))?,
        blocks(&lane(&events, HOST))?
    );
    Ok(())
}

fn hl7_refusal(message: &[u8]) -> Result<Error, Box<dyn std::error::Error>> {
    let wire = mllp::encode(message, mllp::Limit::new(65536)?)?;
    match de_identify(&header(HL7_STACK), &recorded(vec![(MACHINE, wire)]), None) {
        Ok(_) => Err("a copy was made".into()),
        Err(error) => Ok(error),
    }
}

// HL7 2.5.1 ch.2 §2.8 (Z segments are local) and §2.7 (hex escapes); ADR 0037.
#[test]
fn hl7_unknown_segments_and_hex_escapes_refuse_the_file() -> Fallible {
    let local = hl7_refusal(b"MSH|^~\\&|B|L|||1||ORU^R01|1|P|2.5.1\rZPI|1|PATX7781\r")?;
    assert!(matches!(local, Error::Unparsed { event: 1, .. }), "{local}");
    let hex = hl7_refusal(b"MSH|^~\\&|B|L|||1||ORU^R01|1|P|2.5.1\rPID|1||\\X50415458\\\r")?;
    assert!(matches!(hex, Error::Unparsed { .. }));
    let headless = hl7_refusal(b"PID|1||PATX7781\r")?;
    assert!(matches!(headless, Error::Unparsed { .. }));
    Ok(())
}

// ADR 0037: POCT1-A and watched-folder recordings are refused, never partly cleaned.
#[test]
fn unsupported_protocols_are_refused_by_name() {
    let events = recorded(vec![(MACHINE, b"<HEL.R01/>".to_vec())]);
    let poct = de_identify(
        &header(["tcp_server", "poct", "poct", "poct"]),
        &events,
        None,
    );
    assert!(matches!(poct, Err(Error::Unsupported("POCT1-A"))));
    let folder = de_identify(
        &header(["watched_folder", "none", "file_pickup", "hl7_v2"]),
        &events,
        None,
    );
    assert!(matches!(folder, Err(Error::Unsupported(_))));
    let unknown = de_identify(&header(["serial", "x", "y", "z"]), &events, None);
    assert!(matches!(unknown, Err(Error::UnknownStack)));
}

const VIDAS_STACK: [&str; 4] = ["serial", "vidas_literal", "vidas_literal", "vidas_literal"];

fn vidas_config() -> Result<vidas::Config, vidas::Error> {
    vidas::Config::new(
        vidas::Mode::Literal,
        20,
        1920,
        &[],
        vidas::Checksum {
            include_stx: false,
            include_suffix: false,
            hex: vidas::HexOptions {
                uppercase: true,
                accept_either_case: false,
            },
        },
    )
}

const VIDAS_RESULT: &[u8] = b"mtrsl|piPATX7781|pnZORBALIUS QUENTINA|pb1971/03/15|psF|so|si|ci000SMPZ9981|rtTSH|rnThyroid|tt09:00|td2026/01/01|ql|qn8.64|y3ng/mL|qd1|ncvalid|idPC1|xnZEPHYRINE note|";
const VIDAS_CONTROL: &[u8] =
    b"mtrsl|pi|si|ciQC1|rtTSH|tt09:00|td2026/01/01|ql|qn1.20|y3ng/mL|qd1|ncconthwst|";

/// The data bytes of each packet: printable bytes between STX and GS.
fn vidas_bodies(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|b| *b == 0x02)
        .skip(1)
        .map(|packet| {
            packet
                .split(|b| *b == 0x1D)
                .next()
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|b| (0x20..=0x7E).contains(b))
                .collect()
        })
        .collect()
}

fn tag<'a>(body: &'a [u8], name: &[u8]) -> &'a [u8] {
    body.split(|b| *b == b'|')
        .find(|field| field.get(..2) == Some(name))
        .and_then(|field| field.get(2..))
        .unwrap_or_default()
}

// BCI Link 161150-486 C pp. 3-1 to 3-7 (packets and checksum) and 4-2 to 4-8
// (tags), p. 4-5 (controls); ADR 0037.
#[test]
fn vidas_copy_replaces_patient_tags_and_keeps_results_and_controls() -> Fallible {
    let config = vidas_config()?;
    let result = config.encode(VIDAS_RESULT)?;
    let control = config.encode(VIDAS_CONTROL)?;
    let mut parts = vec![(MACHINE, vec![0x05]), (HOST, vec![0x06])];
    split(MACHINE, &result, &mut parts);
    parts.push((HOST, vec![0x06]));
    parts.push((MACHINE, control.clone()));
    parts.push((HOST, vec![0x06]));
    parts.push((MACHINE, vec![0x03, 0x04]));
    let events = recorded(parts);
    let copy = de_identify(&header(VIDAS_STACK), &events, Some(LetterCase::Upper))?;
    assert_clean(&events, &copy, &[]);
    // RS may split a field, so values are looked for in the joined packet data.
    let bodies = vidas_bodies(&lane(&copy.events, MACHINE));
    let before = vidas_bodies(&lane(&events, MACHINE)).concat();
    for value in [
        "PATX7781",
        "ZORBALIUS",
        "QUENTINA",
        "1971/03/15",
        "SMPZ9981",
        "ZEPHYRINE",
    ] {
        assert!(contains(&before, value.as_bytes()));
        assert!(
            !contains(&bodies.concat(), value.as_bytes()),
            "{value} survived"
        );
    }
    let [new_result, new_control] = bodies.as_slice() else {
        return Err("two packets".into());
    };
    // Each packet is exactly what the framer would send for its new body.
    let machine = lane(&copy.events, MACHINE);
    assert!(contains(&machine, &config.encode(new_result)?));
    assert_eq!(new_control.as_slice(), VIDAS_CONTROL);
    for name in [
        b"qn".as_slice(),
        b"y3",
        b"tt",
        b"td",
        b"rt",
        b"nc",
        b"rn",
        b"id",
    ] {
        assert_eq!(tag(new_result, name), tag(VIDAS_RESULT, name));
    }
    assert_eq!(tag(new_result, b"ps"), b"U");
    let birth = tag(new_result, b"pb");
    assert_eq!(
        (birth.len(), birth.get(4), birth.get(7)),
        (10, Some(&b'/'), Some(&b'/'))
    );
    assert_eq!(tag(new_result, b"ci").get(..3), Some(b"000".as_slice()));
    Ok(())
}

const AUTOLYSER_STACK: [&str; 4] = ["serial", "autolyser", "autolyser", "autolyser"];

// Autolyser Rev18 pp.175-179: patient code first, decimal checksum before EOT; ADR 0037.
#[test]
fn autolyser_copy_replaces_the_patient_code_in_reports_and_work_lists() -> Fallible {
    let report = envelope::encode(b"000PATX7781    TS002874   <0.12TSH    8.64".to_vec());
    let work = envelope::encode(b"000PATX7781    TSN0301874 ".to_vec());
    let mut parts = vec![(HOST, vec![envelope::STX]), (MACHINE, vec![envelope::ACK])];
    parts.push((HOST, vec![b'R', envelope::EOT]));
    split(MACHINE, &report, &mut parts);
    parts.extend([
        (HOST, vec![envelope::STX]),
        (MACHINE, vec![envelope::ACK]),
        (HOST, work.clone()),
        (MACHINE, b"Y\x03".to_vec()),
        (HOST, vec![envelope::STX]),
        (MACHINE, vec![envelope::ACK]),
        (HOST, vec![b'R', envelope::EOT]),
        (MACHINE, vec![envelope::NAK]),
    ]);
    let events = recorded(parts);
    let copy = de_identify(&header(AUTOLYSER_STACK), &events, None)?;
    assert_clean(&events, &copy, &["PATX7781"]);
    let machine = lane(&copy.events, MACHINE);
    let new_report = machine
        .get(1..1usize.saturating_add(report.len()))
        .ok_or("report")?;
    let body = envelope::decode(new_report)?;
    let parsed = analyte_autolyser::syntax::Report::parse(body, false)?;
    let original = analyte_autolyser::syntax::Report::parse(envelope::decode(&report)?, false)?;
    assert_eq!(parsed.values, original.values);
    assert_eq!(body.get(..3), Some(b"000".as_slice()));
    assert_eq!(body.get(11..15), Some(b"    ".as_slice()));
    let host = lane(&copy.events, HOST);
    let new_work = host
        .get(4..4usize.saturating_add(work.len()))
        .ok_or("work list")?;
    let work_body = envelope::decode(new_work)?;
    assert_eq!(work_body.get(..15), body.get(..15), "the same patient code");
    assert_eq!(work_body.get(15..), b"TSN0301874 ".get(..));
    Ok(())
}

// Autolyser Rev18 p.175: the machine speaks only when the host asked; ADR 0037.
#[test]
fn autolyser_bytes_nobody_asked_for_refuse_the_file() {
    let events = recorded(vec![(MACHINE, b"000PATX7781    TS000123\x04".to_vec())]);
    let refused = de_identify(&header(AUTOLYSER_STACK), &events, None);
    assert!(matches!(refused, Err(Error::Unparsed { event: 1, .. })));
}

proptest! {
    // ADR 0037: whatever the patient values are, none survives the copy.
    #[test]
    fn random_patient_fields_never_survive(
        id in "[A-Z][A-Z0-9]{7,14}",
        name in "[A-Z][a-z]{6,12}",
    ) {
        let template = String::from_utf8_lossy(&hl7_messages("", "").concat()).into_owned();
        prop_assume!(!template.contains(&id) && !template.contains(&name));
        let astm = astm_upload(&id, &name).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let hl7 = hl7_recording(&id, &name).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for (stack, events) in [(ASTM_STACK, astm), (HL7_STACK, hl7)] {
            let copy = de_identify(&header(stack), &events, None)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let all: Vec<u8> = copy.events.iter().flat_map(|e| e.bytes.clone()).collect();
            prop_assert!(!contains(&all, id.as_bytes()));
            prop_assert!(!contains(&all, name.as_bytes()));
        }
    }
}
