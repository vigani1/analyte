//! ADR 0009 resource boundaries and exact byte preservation.
use analyte_transport::recording::*;
use proptest::prelude::*;
fn header() -> Header {
    Header {
        version: 1,
        stack: ["tcp", "mllp", "hl7_receiver", "hl7"].map(str::to_owned),
        machine: None,
        software: None,
        evidence: Evidence::Synthetic,
        de_identification: "Synthetic; no patient data".into(),
        source: "ADR 0009 synthetic property cases".into(),
        exceptions: Vec::new(),
    }
}
proptest! {
    #[test]
    fn adr0009_round_trip(bytes in prop::collection::vec(any::<u8>(), 0..4097)) {
        let limits = Limits::default();
        let event = Event { direction: Direction::MachineToHost, time_ms: 0, bytes, link: None };
        let mut wire = Vec::new();
        write_header(&mut wire, &header(), limits)?;
        write_event(&mut wire, &event, limits)?;
        let (got, events) = read(wire.as_slice(), limits)?;
        prop_assert_eq!(got, header());
        prop_assert_eq!(events, vec![event]);
    }
    #[test]
    fn adr0009_arbitrary_input_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..32768)) {
        let _ = read(bytes.as_slice(), Limits::default());
    }
}
#[test]
fn adr0009_invalid_line_and_each_limit_are_errors() -> Result<(), Box<dyn std::error::Error>> {
    let mut wire = Vec::new();
    let limits = Limits::default();
    write_header(&mut wire, &header(), limits)?;
    let header_len = wire.len();
    write_event(
        &mut wire,
        &Event {
            direction: Direction::MachineToHost,
            time_ms: 0,
            bytes: vec![1, 2, 3],
            link: None,
        },
        limits,
    )?;
    for bound in [
        Limits {
            file_bytes: header_len,
            ..limits
        },
        Limits {
            line_bytes: 2,
            ..limits
        },
        Limits {
            events: 0,
            ..limits
        },
        Limits {
            event_bytes: 2,
            ..limits
        },
    ] {
        assert!(read(wire.as_slice(), bound).is_err());
    }
    wire.extend_from_slice(b"invalid\n");
    assert_eq!(
        read(wire.as_slice(), limits)
            .err()
            .ok_or("error missing")?
            .line,
        3
    );
    Ok(())
}
