//! Qualifies edition-specific O21 encoding and O22 response decisions.
use analyte_hl7::order::*;
use analyte_hl7::{ControlId, Message, Options, Version};

fn order<'a>(revision: Option<&'a [u8]>) -> Result<Order<'a>, Error> {
    let mut test = Code::new(b"T1", b"Synthetic", b"L")?;
    if let Some(revision) = revision {
        test = test.with_version(revision)?;
    }
    Ok(Order::New {
        placer: OrderId::new(b"001", b"LAB")?,
        filler: None,
        test,
        specimen: Specimen {
            sample: SampleId::new(b"000aB")?,
            field: SampleField::Placer1,
            source: Some(Code::new(b"SER", b"", b"L")?.with_version(b"spec-r1")?),
        },
        priority: Priority::Stat,
        timing: Timing {
            start: Some(Timestamp::new(b"202611161300")?),
            end: None,
            priority_version: Some(b"prio-r1"),
        },
    })
}
fn header(version: Version) -> Result<Header<'static>, Error> {
    Ok(Header {
        application: b"HOST",
        facility: b"LAB",
        peer_application: b"MACHINE",
        peer_facility: b"LAB",
        control_id: ControlId::new(b"sent1")?,
        timestamp: Timestamp::new(b"20261116120000+0100")?,
        version,
        processing: Processing::Training,
    })
}
fn sent(version: Version) -> Result<OrderMessage, Error> {
    oml(
        &[order(Some(b"test-r1"))?],
        Subject::Unspecified,
        header(version)?,
        Options::default(),
    )
}
#[test]
fn hl7_2_6_s4_4_6_s4_5_3_4_s4_5_4_s7_4_3_exact_bytes() -> Result<(), Error> {
    let sent = sent(Version::V26)?;
    assert_eq!(sent.message().raw(), concat!(
        "MSH|^~\\&|HOST|LAB|MACHINE|LAB|20261116120000+0100||OML^O21^OML_O21|sent1|T|2.6||||||ASCII\r",
        "ORC|NW|001^LAB||||F|\r",
        "TQ1|1||||||202611161300||S^^HL70485^^^^prio-r1\r",
        "OBR|1|001^LAB||T1^Synthetic^L^^^^test-r1|||||||||||||||||||||||\r",
        "SPM|1|000aB||SER^^L^^^^spec-r1\r").as_bytes());
    Ok(())
}
#[test]
fn hl7_2_6_s2_a_13_7_revisions_are_explicit() -> Result<(), Error> {
    let version = Version::V26;
    assert!(
        oml(
            &[order(None)?],
            Subject::Unspecified,
            header(version)?,
            Options::default()
        )
        .is_err()
    );
    let mut work = order(Some(b"r1"))?;
    if let Order::New { timing, .. } = &mut work {
        timing.priority_version = None;
    }
    assert!(
        oml(
            &[work],
            Subject::Unspecified,
            header(version)?,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn hl7_2_6_s4_4_7_s2_14_8_response_outcomes_and_correlation() -> Result<(), Error> {
    let sent = sent(Version::V26)?;
    for (control, expected) in [("OK", true), ("UA", false)] {
        let bytes = format!(
            "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611161201||ORL^O22^ORL_O22|reply1|T|2.6\rMSA|AA|sent1\rPID|1||P1^^^LAB||Synthetic\rORC|{control}|001^LAB\r"
        );
        let message = Message::parse(bytes.as_bytes(), Options::default())?;
        let response = OrderResponse::new(&message, &sent)?;
        assert_eq!(
            matches!(response.decisions()[0].outcome(), Outcome::Accepted),
            expected
        );
        if !expected {
            assert!(matches!(
                response.decisions()[0].outcome(),
                Outcome::Rejected { .. }
            ));
        }
        for malformed in [
            bytes.replace("MSA|AA|sent1", "MSA|AA|other"),
            bytes.replace("MSA|AA|sent1", "MSA|AA|sent1|||D"),
            bytes.replace("ORL^O22^ORL_O22", "ORL^O53^ORL_O53"),
        ] {
            let message = Message::parse(malformed.as_bytes(), Options::default())?;
            assert!(OrderResponse::new(&message, &sent).is_err());
        }
    }
    Ok(())
}
#[test]
fn hl7_2_6_s2_a_6_77_withdrawn_ce_ts_and_legacy_writer_refused() -> Result<(), Error> {
    let version = Version::V26;
    assert!(
        orm(
            &[order(Some(b"r1"))?],
            Subject::Unspecified,
            header(version)?,
            Options::default()
        )
        .is_err()
    );
    let sent = sent(Version::V26)?;
    for row in sent.message().segments().filter(|s| s.id() == b"OBR") {
        for field in [5, 6, 14, 15, 27] {
            assert!(row.field(field).is_none_or(|f| f.raw().is_empty()));
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s4_4_6_7_s4_5_1_4_s2_a_14_5_order_qualification() -> Result<(), Error> {
    let version = Version::V282;
    let sent = sent(version)?;
    let expected = concat!(
        "MSH|^~\\&|HOST|LAB|MACHINE|LAB|20261116120000+0100||OML^O21^OML_O21|sent1|T|2.8.2||||||ASCII\r",
        "ORC|NW|001^LAB||||F|\r",
        "TQ1|1||||||202611161300||S^^HL70485^^^^prio-r1\r",
        "OBR|1|001^LAB||T1^Synthetic^L^^^^test-r1|||||||||||||||||||||||\r",
        "SPM|1|000aB||SER^^L^^^^spec-r1\r"
    );
    assert_eq!(sent.message().raw(), expected.as_bytes());
    let grouped = context(&sent, b"g1", None, Options::default())?;
    assert!(
        grouped
            .message()
            .raw()
            .windows(b"ORC|NW|001^LAB||g1&LAB|".len())
            .any(|w| w == b"ORC|NW|001^LAB||g1&LAB|")
    );
    for identifier_type in [None, Some(b"MR".as_slice())] {
        let result = oml(
            &[order(Some(b"test-r1"))?],
            Subject::Patient(Patient {
                identifier: b"P1",
                namespace: b"LAB",
                family_name: b"Synthetic",
                identifier_type,
            }),
            header(version)?,
            Options::default(),
        );
        assert_eq!(result.is_ok(), identifier_type.is_some());
        if let Ok(message) = result {
            assert!(
                message
                    .message()
                    .raw()
                    .windows(b"PID|1||P1^^^LAB^MR||Synthetic\r".len())
                    .any(|w| w == b"PID|1||P1^^^LAB^MR||Synthetic\r")
            );
        }
    }
    assert!(
        oml(
            &[order(None)?],
            Subject::Unspecified,
            header(version)?,
            Options::default()
        )
        .is_err()
    );
    for control in ["OK", "UA"] {
        let raw = format!(
            "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611161201||ORL^O22^ORL_O22|reply1|T|2.8.2\rMSA|AA|sent1\rPID|1||P1^^^LAB^MR||Synthetic\rORC|{control}|001^LAB\r"
        );
        let message = Message::parse(raw.as_bytes(), Options::default())?;
        let response = OrderResponse::new(&message, &sent)?;
        assert!(match control {
            "OK" => matches!(response.decisions()[0].outcome(), Outcome::Accepted),
            _ => matches!(response.decisions()[0].outcome(), Outcome::Rejected { .. }),
        });
        for invalid in [
            raw.replace("MSA|AA|sent1", "MSA|AA|other"),
            raw.replace("ORL^O22^ORL_O22", "ORL^O22^ORL_O41"),
            raw.replace("^LAB^MR", "^LAB"),
            raw.replace("MSA|AA|sent1", "MSA|AA|sent1|withdrawn"),
            raw.replace("MSA|AA|sent1", "MSA|AA|sent1|\"\""),
            raw.replace("MSA|AA|sent1", "MSA|AA|sent1\rERR|\"\"||207|E"),
        ] {
            let message = Message::parse(invalid.as_bytes(), Options::default())?;
            assert!(OrderResponse::new(&message, &sent).is_err());
        }
        for (segment, fields) in [
            ("ORC", &[7_usize][..]),
            ("OBR", &[5_usize, 6, 14, 15, 27][..]),
        ] {
            for field in fields {
                let mut values = vec![""; *field];
                if segment == "ORC" {
                    values[0] = control;
                    values[1] = "001^LAB";
                } else {
                    values[1] = "001^LAB";
                    values[3] = "T1^Synthetic^L^^^^test-r1";
                }
                values[*field - 1] = "withdrawn";
                let invalid = if segment == "ORC" {
                    raw.replace(
                        &format!("ORC|{control}|001^LAB\r"),
                        &format!("ORC|{}\r", values.join("|")),
                    )
                } else {
                    format!("{raw}OBR|{}\r", values.join("|"))
                };
                let message = Message::parse(invalid.as_bytes(), Options::default())?;
                assert!(OrderResponse::new(&message, &sent).is_err());
            }
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_a_13_s4_5_4_7_reply_datatypes_and_name_bound() -> Result<(), Error> {
    let version = Version::V282;
    let sent = sent(version)?;
    let raw = concat!(
        "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611161201||ORL^O22^ORL_O22|reply1|T|2.8.2\rMSA|AA|sent1\rPID|1||P1^^^LAB^MR||Synthetic\r",
        "ORC|OK|001^LAB\rTQ1|1||||||202611161300||S^^HL70485^^^^prio-r1\r",
        "OBR|1|001^LAB||T1^Synthetic^L^^^^test-r1\rSPM|1|000aB||SER^^L^^^^spec-r1\r"
    );
    let message = Message::parse(raw.as_bytes(), Options::default())?;
    OrderResponse::new(&message, &sent)?;
    for invalid in [
        raw.replace("202611161300||", "202611161300^M||"),
        raw.replace("L^^^^test-r1", "L"),
        raw.replace("HL70485^^^^prio-r1", "HL70485"),
        raw.replace("L^^^^spec-r1", "L"),
    ] {
        let message = Message::parse(invalid.as_bytes(), Options::default())?;
        assert!(OrderResponse::new(&message, &sent).is_err());
    }
    let mut work = order(Some(b"r1"))?;
    if let Order::New { test, .. } = &mut work {
        *test = Code::new(b"T1", b"", b"ABCDEFGHIJKLM")?.with_version(b"r1")?;
    }
    assert!(
        oml(
            &[work],
            Subject::Unspecified,
            header(version)?,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}

// HL7 2.5.1/2.6 section 4.4.7 and 3.4.2; 2.8.2 section 3.3.2.
#[test]
fn hl7_251_26_s4_4_7_required_pid_content_in_order_reply() -> Result<(), Error> {
    for version in [Version::V26, Version::V282] {
        let sent = sent(version)?;
        for pid in [
            "PID",
            "PID|1",
            "PID|1||\"\"||Synthetic",
            "PID|1||P1",
            "PID|1||P1||\"\"",
            "PID|1||^^^LAB||Synthetic",
        ] {
            let raw = format!(
                "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611161201||ORL^O22^ORL_O22|reply1|T|{}\rMSA|AA|sent1\r{pid}\rORC|OK|001^LAB\r",
                version.as_str()
            );
            let message = Message::parse(raw.as_bytes(), Options::default())?;
            assert!(
                OrderResponse::new(&message, &sent).is_err(),
                "{version:?}: {pid}"
            );
            assert_eq!(message.encode(), raw.as_bytes());
        }
    }
    Ok(())
}
