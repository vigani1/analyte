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
fn header() -> Result<Header<'static>, Error> {
    Ok(Header {
        application: b"HOST",
        facility: b"LAB",
        peer_application: b"MACHINE",
        peer_facility: b"LAB",
        control_id: ControlId::new(b"sent1")?,
        timestamp: Timestamp::new(b"20261116120000+0100")?,
        version: Version::V26,
        processing: Processing::Training,
    })
}
fn sent() -> Result<OrderMessage, Error> {
    oml(
        &[order(Some(b"test-r1"))?],
        Subject::Unspecified,
        header()?,
        Options::default(),
    )
}
#[test]
fn hl7_2_6_s4_4_6_s4_5_3_4_s4_5_4_s7_4_3_exact_bytes() -> Result<(), Error> {
    let sent = sent()?;
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
    assert!(
        oml(
            &[order(None)?],
            Subject::Unspecified,
            header()?,
            Options::default()
        )
        .is_err()
    );
    let mut work = order(Some(b"r1"))?;
    if let Order::New { timing, .. } = &mut work {
        timing.priority_version = None;
    }
    assert!(oml(&[work], Subject::Unspecified, header()?, Options::default()).is_err());
    Ok(())
}
#[test]
fn hl7_2_6_s4_4_7_s2_14_8_response_outcomes_and_correlation() -> Result<(), Error> {
    let sent = sent()?;
    for (control, expected) in [("OK", true), ("UA", false)] {
        let bytes = format!(
            "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611161201||ORL^O22^ORL_O22|reply1|T|2.6\rMSA|AA|sent1\rPID|1\rORC|{control}|001^LAB\r"
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
    assert!(
        orm(
            &[order(Some(b"r1"))?],
            Subject::Unspecified,
            header()?,
            Options::default()
        )
        .is_err()
    );
    let sent = sent()?;
    for row in sent.message().segments().filter(|s| s.id() == b"OBR") {
        for field in [5, 6, 14, 15, 27] {
            assert!(row.field(field).is_none_or(|f| f.raw().is_empty()));
        }
    }
    Ok(())
}
