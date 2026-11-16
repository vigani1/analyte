//! Regression scenarios for syntax.
use super::*;

#[test]
fn lis02_6_2_bootstrap_and_5_4_6_message_delimiters() -> Result<(), Error> {
    let messages = parse(
        b"h;~:!;control\rP;1;001Ab\rL;1;N\rH|\\^&|two\rL|1\r",
        Limits::default(),
    )?;
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages.first().map(|m| m.delimiters().bytes()),
        Some(*b";~:!")
    );
    assert!(parse(b"H|\\^&|\rP;1\rL|1\r", Limits::default()).is_err());
    Ok(())
}

#[test]
fn lis02_5_4_5_escapes_never_change_boundaries() -> Result<(), Error> {
    let wire = b"H|\\^&|\rP|1|&F&&S&&R&&E&&Zx|^\\y&&X0D&&H&bold&N&&unknown&\rL|1\r";
    let messages = parse(wire, Limits::default())?;
    let field = messages
        .first()
        .and_then(|m| m.records().get(1))
        .and_then(|r| r.field(3));
    assert_eq!(field.map(|f| f.repeats().len()), Some(1));
    assert_eq!(
        field.and_then(|f| f.repeats().first()).map(Vec::len),
        Some(1)
    );
    assert_eq!(
        field
            .and_then(|f| f.repeats().first())
            .and_then(|r| r.first())
            .map(analyte_astm::Component::text),
        Some("|^\\&&Zx|^\\y&&X0D&&H&bold&N&&unknown&")
    );
    assert_eq!(
        messages.first().map(analyte_astm::Message::encode),
        Some(wire.to_vec())
    );
    assert!(matches!(
        parse(b"H|\\^&|\rP|1|&F\rL|1\r", Limits::default()),
        Err(Error::Escape { field: 0, .. })
    ));
    Ok(())
}

#[test]
fn lis02_5_4_7_through_9_null_positions_and_deletion() -> Result<(), Error> {
    let messages = parse(b"H|\\^&|\rP|1||\"\"|^a^\\b\\|\rL|1\r", Limits::default())?;
    let record = messages.first().and_then(|m| m.records().get(1));
    assert_eq!(
        record
            .and_then(|r| r.field(3))
            .map(analyte_astm::Field::presence),
        Some(Presence::Empty)
    );
    assert_eq!(
        record
            .and_then(|r| r.field(4))
            .map(analyte_astm::Field::presence),
        Some(Presence::Delete)
    );
    assert_eq!(
        record.and_then(|r| r.field(5)).map(|f| f.repeats().len()),
        Some(3)
    );
    assert!(record.and_then(|r| r.field(7)).is_none());
    Ok(())
}

#[test]
fn lis02_4_1_parents_and_context_closure() -> Result<(), Error> {
    let messages = parse(
        b"H|\\^&|\rP|1\rO|1\rR|1\rC|1|x\rM|1|y\rR|2\rO|2\rR|1\rQ|1\rS|1\rL|1\r",
        Limits::default(),
    )?;
    let records = messages
        .first()
        .map(analyte_astm::Message::records)
        .unwrap_or_default();
    assert_eq!(
        records.get(3).and_then(analyte_astm::Record::parent),
        Some(2)
    );
    assert_eq!(
        records.get(4).and_then(analyte_astm::Record::parent),
        Some(3)
    );
    assert_eq!(
        records.get(5).and_then(analyte_astm::Record::parent),
        Some(4)
    );
    assert_eq!(
        records.get(8).and_then(analyte_astm::Record::sequence),
        Some(1)
    );
    for wire in [
        b"H|\\^&|\rO|1\rL|1\r".as_slice(),
        b"H|\\^&|\rP|1\rR|1\rL|1\r",
        b"H|\\^&|\rP|1\rQ|1\rO|1\rL|1\r",
        b"H|\\^&|\rL|1\rC|1\r",
        b"H|\\^&|\rH|\\^&|\rL|1\r",
        b"H|\\^&|\r",
    ] {
        assert!(matches!(
            parse(wire, Limits::default()),
            Err(Error::Hierarchy { .. })
        ));
    }
    Ok(())
}

#[test]
fn lis02_5_6_7_sequence_numbers_reset_with_context() {
    for wire in [
        b"H|\\^&|\rP|0\rL|1\r".as_slice(),
        b"H|\\^&|\rP|1\rP|1\rL|1\r",
        b"H|\\^&|\rP|1^2\rL|1\r",
        b"H|\\^&|\rP|999999999999999999999999999999999\rL|1\r",
    ] {
        assert!(matches!(
            parse(wire, Limits::default()),
            Err(Error::Sequence { .. })
        ));
    }
}

#[test]
fn lis02_5_1_latin1_is_not_utf8_or_lossy() -> Result<(), Error> {
    let wire = b"H|\\^&|\rP|1|\xe9\rL|1\r";
    let messages = parse(wire, Limits::default())?;
    assert_eq!(
        messages
            .first()
            .and_then(|m| m.records().get(1))
            .and_then(|r| r.field(3))
            .and_then(|f| f.repeats().first())
            .and_then(|r| r.first())
            .map(analyte_astm::Component::text),
        Some("é")
    );
    assert!(matches!(
        parse_with(wire, Limits::default(), |b| std::str::from_utf8(b)
            .ok()
            .map(str::to_owned)),
        Err(Error::Decode { .. })
    ));
    for byte in [0, 10, 127, 255] {
        let mut bad = b"H|\\^&|\rP|1|".to_vec();
        bad.push(byte);
        bad.extend(b"\rL|1\r");
        assert!(matches!(
            parse(&bad, Limits::default()),
            Err(Error::Character { .. })
        ));
    }
    Ok(())
}

#[test]
fn lis02_5_2_5_3_limits_reject_instead_of_truncating() {
    let wire = b"H|\\^&|\rP|1|a^b\\c\rL|1\r";
    for limits in [
        Limits {
            bytes: 1,
            ..Limits::default()
        },
        Limits {
            record: 3,
            ..Limits::default()
        },
        Limits {
            fields: 2,
            ..Limits::default()
        },
        Limits {
            repeats: 1,
            ..Limits::default()
        },
        Limits {
            components: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(parse(wire, limits), Err(Error::Limit { .. })));
    }
}

#[test]
fn lis02_9_9_all_result_codes_remain_distinct() {
    let codes = b"CPFXISMRNQVW";
    let statuses: Vec<_> = codes
        .iter()
        .map(|b| ResultStatus::from_bytes(&[*b]))
        .collect();
    for (i, status) in statuses.iter().enumerate() {
        assert!(!statuses.iter().take(i).any(|s| s == status));
    }
    assert_eq!(
        ResultStatus::from_bytes(b"future"),
        ResultStatus::Other(b"future".to_vec())
    );
}

#[test]
fn lis02_5_4_serialize_selected_delimiters() -> Result<(), Error> {
    let messages = parse(b"H|\\^&|\rP|1|a;b&F&c^d\\e\rL|1\r", Limits::default())?;
    let target = Delimiters::new(b';', b'~', b':', b'!')?;
    let serialized = messages
        .first()
        .map(|m| m.encode_with(target))
        .transpose()?
        .unwrap_or_default();
    assert_eq!(serialized, b"H;~:!;\rP;1;a!F!b|c:d~e\rL;1\r");
    let reparsed = parse(&serialized, Limits::default())?;
    assert_eq!(
        reparsed
            .first()
            .and_then(|m| m.records().get(1))
            .map(analyte_astm::Record::kind),
        Some(Kind::Patient)
    );
    Ok(())
}

proptest! {
    #[test]
    fn lis02_5_7_parse_serialize_identity(value in "[a-zA-Z0-9 ^\\\\]{0,100}") {
        let wire = format!("H|\\^&|\rP|1|{value}\rL|1\r");
        let parsed = parse(wire.as_bytes(), Limits::default());
        prop_assert!(parsed.is_ok());
        if let Ok(messages) = parsed {
            let serialized: Vec<u8> = messages.iter().flat_map(analyte_astm::Message::encode).collect();
            prop_assert_eq!(&serialized, wire.as_bytes());
            prop_assert_eq!(parse(&serialized, Limits::default()), Ok(messages));
        }
    }
    #[test]
    fn lis02_5_hostile_bytes_are_bounded(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let _ = parse(&bytes, Limits { bytes: 1024, ..Limits::default() });
    }
}

#[test]
fn lis02_6_12_and_12_3_processing_and_termination_codes() {
    use analyte_astm::{ProcessingId, TerminationCode};
    for (raw, expected) in [
        (b"P".as_slice(), ProcessingId::Production),
        (b"T", ProcessingId::Training),
        (b"D", ProcessingId::Debug),
        (b"Q", ProcessingId::QualityControl),
    ] {
        assert_eq!(ProcessingId::from_bytes(raw), expected);
    }
    for (raw, expected) in [
        (b"".as_slice(), TerminationCode::Normal),
        (b"N", TerminationCode::Normal),
        (b"T", TerminationCode::SenderAbort),
        (b"R", TerminationCode::ReceiverAbort),
        (b"E", TerminationCode::SystemError),
        (b"Q", TerminationCode::QueryError),
        (b"I", TerminationCode::NoInformation),
        (b"F", TerminationCode::Finished),
    ] {
        assert_eq!(TerminationCode::from_bytes(raw), expected);
    }
    assert_eq!(
        ProcessingId::from_bytes(b"p"),
        ProcessingId::Other(b"p".to_vec())
    );
    assert_eq!(
        TerminationCode::from_bytes(b"?"),
        TerminationCode::Other(b"?".to_vec())
    );
}

#[test]
fn lis02_sections_6_through_14_read_only_field_positions() -> Result<(), Error> {
    let messages = parse(b"H|\\^&|3|4|5|6|7|8|9|10|11|12|13|14\rP|1|3|4|5|6|7|8|9|10|11|12|13|14|15|16|17|18|19|20|21|22|23|24|25|26|27|28|29|30|31|32|33|34|35\rO|1|3|4|5|6|7|8|9|10|11|12|13|14|15|16|17|18|19|20|21|22|23|24|25|26|27|28|29|30|31\rR|1|3|4|5|6|7|8|9|10|11|12|13|14\rC|1|3|4|5\rM|1|3|4|5|6|7|8\rQ|1|3|4|5|6|7|8|9|10|11|12|13\rS|1|3|4|5|6|7|8|9|10|11|12|13|14|15|16|17|18|19|20|21\rL|1|3\r", Limits::default())?;
    let records = messages
        .first()
        .map(analyte_astm::Message::records)
        .unwrap_or_default();
    check_header(records)?;
    check_patient(records)?;
    check_order(records)?;
    check_result(records)?;
    check_comment(records)?;
    check_manufacturer(records)?;
    check_query(records)?;
    check_scientific(records)?;
    check_terminator(records)?;
    Ok(())
}
