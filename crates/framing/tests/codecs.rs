//! Synthetic conformance cases, independent of vendor behavior.
use analyte_framing::{astm as a, mllp as m};
use proptest::prelude::*;

fn astm(bytes: &[u8]) -> Vec<a::Event> {
    let mut d = a::Decoder::new(a::Limit::default(), a::HexPolicy::Strict);
    bytes.iter().filter_map(|b| d.push(*b)).collect()
}
#[test]
fn lis01_6_3_3_checksum_sums_raw_digit_through_etx() -> Result<(), a::Error> {
    let frames: Vec<_> =
        a::encode(&[0x80, 13], a::FrameNumber::FIRST, a::Limit::default())?.collect();
    assert_eq!(frames, vec![vec![2, b'1', 0x80, 13, 3, b'C', b'1', 13, 10]]);
    assert!(matches!(
        astm(&frames.concat()).first(),
        Some(a::Event::Frame(_))
    ));
    Ok(())
}
#[test]
fn lis01_6_3_2_number_rollover() -> Result<(), a::Error> {
    let mut n = a::FrameNumber::FIRST;
    for expected in b"123456701" {
        assert_eq!(n.ascii(), *expected);
        assert_eq!(n.value(), expected.wrapping_sub(b'0'));
        n = n.next();
    }
    assert_eq!(a::FrameNumber::MAX.value(), 7);
    for byte in 0..=255 {
        assert_eq!(
            a::FrameNumber::new(byte).is_ok(),
            (b'0'..=b'7').contains(&byte)
        );
    }
    Ok(())
}
#[test]
fn lis01_6_3_and_6_6_defects_and_recovery() {
    let cases = [
        (b"\x028".as_slice(), a::Error::BadFrameNumber(b'8')),
        (b"\x021\n".as_slice(), a::Error::ControlInText(10)),
        (b"\x021\x03a".as_slice(), a::Error::LowercaseChecksum),
        (b"\x021\x03Z".as_slice(), a::Error::NonHexChecksum(b'Z')),
        (
            b"\x021\x0300\r".as_slice(),
            a::Error::BadChecksum {
                computed: 0x34,
                received: 0,
            },
        ),
        (b"\x021\x0334X".as_slice(), a::Error::MissingTrailer(13)),
        (b"\x021\x0334\rX".as_slice(), a::Error::MissingTrailer(10)),
    ];
    for (wire, expected) in cases {
        let mut bytes = wire.to_vec();
        bytes.extend_from_slice(b"\x021\x0334\r\n");
        let events = astm(&bytes);
        assert!(
            matches!(events.first(), Some(a::Event::Rejected { error, raw }) if *error == expected && raw == wire)
        );
        assert!(matches!(events.last(), Some(a::Event::Frame(_))));
    }
}
#[test]
fn lis01_6_3_1_standard_total_boundary() -> Result<(), a::Error> {
    let record = vec![b'A'; 63_994];
    let frames: Vec<_> = a::encode(&record, a::FrameNumber::FIRST, a::Limit::default())?.collect();
    assert_eq!(
        frames.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![64_000, 8]
    );
    let events = astm(&frames.concat());
    assert!(matches!(events.first(), Some(a::Event::Frame(f)) if !f.is_final()));
    assert!(matches!(events.last(), Some(a::Event::Frame(f)) if f.is_final()));
    Ok(())
}
#[test]
fn lis01_6_3_3_lowercase_policy_retains_wire() {
    let bytes = b"\x021z\x03ae\r\n";
    let mut d = a::Decoder::new(a::Limit::default(), a::HexPolicy::AcceptLowercase);
    let events: Vec<_> = bytes.iter().filter_map(|b| d.push(*b)).collect();
    assert!(matches!(events.first(), Some(a::Event::Frame(f)) if f.raw() == bytes));
}
#[test]
fn mllp_r1_1_2_specific_defects_and_restart() -> Result<(), m::Error> {
    let mut d = m::Decoder::new(m::Limit::new(2)?);
    for (bytes, expected) in [
        (b"x".as_slice(), m::Error::Outside),
        (b"\x1c".as_slice(), m::Error::EndWithoutStart),
        (b"\x0ba\x0b".as_slice(), m::Error::Interrupted),
    ] {
        let events: Vec<_> = bytes.iter().filter_map(|b| d.push(*b)).collect();
        assert!(
            matches!(events.last(), Some(m::Event::Rejected { error, .. }) if *error == expected)
        );
        let _ = d.finish();
    }
    for (wire, expected) in [
        (b"\x0babc".as_slice(), m::Error::TooLong),
        (b"\x0b\x1cX".as_slice(), m::Error::MissingCr),
        (b"\x0b\n".as_slice(), m::Error::InvalidByte(10)),
    ] {
        let events: Vec<_> = wire.iter().filter_map(|b| d.push(*b)).collect();
        assert!(
            matches!(events.last(), Some(m::Event::Rejected { error, raw }) if *error == expected && raw == wire)
        );
    }
    Ok(())
}
proptest! {
    #[test]
    fn lis01_6_3_roundtrip_all_chunk_boundaries(payload in prop::collection::vec(any::<u8>().prop_filter("LIS01 permitted text", |b| !matches!(b, 1..=6 | 10 | 16..=23)), 0..150)) {
        let limit = a::Limit::new(17)?;
        let frames: Vec<_> = a::encode(&payload, a::FrameNumber::FIRST, limit)?.collect();
        let bytes = frames.concat();
        let expected = astm(&bytes);
        let recovered: Vec<_> = expected.iter().filter_map(|e| if let a::Event::Frame(f) = e {Some(f.text())} else {None}).flatten().copied().collect();
        prop_assert_eq!(recovered, payload);
        for split in 0..=bytes.len() {
            let (left,right) = bytes.split_at(split);
            let mut d = a::Decoder::new(a::Limit::default(), a::HexPolicy::Strict);
            let events: Vec<_> = left.iter().chain(right).filter_map(|b| d.push(*b)).collect();
            prop_assert_eq!(&events, &expected);
        }
    }
    #[test]
    fn mllp_r1_1_2_roundtrip_chunk_boundaries(payload in prop::collection::vec(any::<u8>().prop_filter("MLLP repertoire", |b| *b > 31 || *b == 13), 0..100)) {
        let limit = m::Limit::new(100)?;
        let raw = m::encode(&payload, limit)?;
        let bytes = [raw.clone(), raw.clone()].concat();
        for split in 0..=bytes.len() {
            let (left,right) = bytes.split_at(split);
            let mut d = m::Decoder::new(limit);
            let events: Vec<_> = left.iter().chain(right).filter_map(|b| d.push(*b)).collect();
            prop_assert_eq!(events, vec![m::Event::Block(raw.clone()), m::Event::Block(raw.clone())]);
        }
    }
    #[test]
    fn analyte_garbage_bounded_and_exactly_accounted(bytes in prop::collection::vec(any::<u8>(), 0..5000)) {
        let mut a = a::Decoder::new(a::Limit::new(16)?, a::HexPolicy::Strict);
        let mut m = m::Decoder::new(m::Limit::new(16)?);
        let mut ar = Vec::new(); let mut mr = Vec::new();
        for b in &bytes {
            if let Some(e) = a.push(*b) { match e { a::Event::Frame(f) => ar.extend_from_slice(f.raw()), a::Event::Rejected { raw, .. } => ar.extend(raw), a::Event::Outside(b) => ar.push(b) } }
            if let Some(e) = m.push(*b) { match e { m::Event::Block(raw) | m::Event::Rejected { raw, .. } => mr.extend(raw) } }
            prop_assert!(a.pending().len() <= 16); prop_assert!(m.pending().len() <= 19);
        }
        ar.extend_from_slice(a.pending()); mr.extend_from_slice(m.pending());
        prop_assert_eq!(&ar, &bytes); prop_assert_eq!(&mr, &bytes);
    }
}

#[test]
fn lis01_6_3_1_overlong_and_incomplete_evidence() -> Result<(), a::Error> {
    let mut d = a::Decoder::new(a::Limit::new(8)?, a::HexPolicy::Strict);
    let wire = b"\x021AAAAAAA";
    let events: Vec<_> = wire.iter().filter_map(|b| d.push(*b)).collect();
    assert!(
        matches!(events.first(), Some(a::Event::Rejected {error: a::Error::TooLong, raw}) if raw == wire.get(..8).unwrap_or_default())
    );
    assert_eq!(events.last(), Some(&a::Event::Outside(b'A')));
    assert!(d.pending().is_empty());
    assert!(d.push(2).is_none());
    assert!(
        matches!(d.finish(), Some(a::Event::Rejected {error: a::Error::Incomplete, raw}) if raw == vec![2])
    );
    assert!(d.finish().is_none());
    Ok(())
}

#[test]
fn lis01_6_4_link_control_inside_frame_is_reported_not_swallowed() {
    for control in [a::ENQ, a::EOT, a::ACK, a::NAK] {
        let wire = [a::STX, b'1', b'A', control];
        let events = astm(&wire);
        assert!(matches!(
            events.as_slice(),
            [a::Event::Rejected { error: a::Error::LinkControlInFrame(c), raw }]
                if *c == control && raw.as_slice() == wire
        ));
    }
}

#[test]
fn lis01_6_3_1_2_adr0003_rejected_frames_retain_stx_through_checksum_cr_lf() -> Result<(), a::Error>
{
    // Different defect locations obey the same envelope rule; no checksum special case.
    for wire in [
        b"\x021A\x0300\r\n".as_slice(),
        b"\x028A\x0300\r\n",
        b"\x021A\x03ZZ\r\n",
        b"\x021A\x0375X\n",
    ] {
        let events = astm(wire);
        assert!(matches!(events.as_slice(), [a::Event::Rejected { raw, .. }] if raw == wire));
    }
    let mut decoder = a::Decoder::new(a::Limit::new(8)?, a::HexPolicy::Strict);
    let wire = b"\x028ABCDEF";
    let events: Vec<_> = wire.iter().filter_map(|b| decoder.push(*b)).collect();
    assert_eq!(
        events,
        vec![a::Event::Rejected {
            error: a::Error::TooLong,
            raw: wire.to_vec()
        }]
    );
    assert!(decoder.pending().is_empty());
    assert_eq!(decoder.push(b'Z'), Some(a::Event::Outside(b'Z')));
    Ok(())
}
