//! Tests exact LAW requested and applied priority preservation.
use super::*;

// TF-2b 2.2.4.10 and C.14: requested and applied priorities retain their supplied coding.
#[test]
fn law_tf2b_c14_requested_and_applied_priority_preserve_exact_code() -> TestResult {
    for code in [
        b"S^Urgent^HL70485".as_slice(),
        b"vendor-7^Bench priority^99LAB",
    ] {
        let capabilities = law::Capabilities {
            instructions: law::PeerInstructions {
                priority: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let awos = law::Awos::new(
            b"awos-1",
            b"sample&Local",
            b"SER^^HL70487",
            b"P",
            b"tube",
            TEST.as_bytes(),
            law::WorkAction::New,
        )?
        .with_instructions(law::Instructions {
            priority: Some(code),
            ..Default::default()
        });
        let output = law::work_order(
            &[awos],
            None,
            law::Address {
                application: b"M",
                facility: b"L",
                peer_application: b"A",
                peer_facility: b"L",
            },
            analyte_hl7::Response {
                control_id: law::control_id(b"priority")?,
                timestamp: b"20260101120000+0000",
            },
            capabilities,
            options(),
        )?;
        let tq1 = output.segments().find(|s| s.id() == b"TQ1").ok_or("TQ1")?;
        assert_eq!(tq1.field(9).ok_or("priority")?.raw(), code);
        let wire = result_wire("")?.replace(
            "ORC|SC||||CM\r",
            &format!("ORC|SC||||CM\rTQ1|||||||||{}\r", std::str::from_utf8(code)?),
        );
        let message = law::parse_message(wire.as_bytes(), options())?;
        let results = law::Results::with_profile(&message, Profile::receive().with_priority(true))?;
        let applied = results
            .attached()
            .iter()
            .find(|a| a.segment.id() == b"TQ1")
            .ok_or("applied priority")?;
        assert_eq!(applied.segment.field(9).ok_or("priority")?.raw(), code);
    }
    Ok(())
}

// TF-2b C.13-8/C.13-9: no clinical permission is inferred from display text.
#[test]
fn law_tf2b_c13_9_repeat_status_output_preserves_profiled_tuple() -> TestResult {
    for code in [
        b"O^Original, first run^HL70389".as_slice(),
        b"R^Repeated without dilution^HL70389",
        b"D^Repeated with dilution^HL70389",
        b"F^Reflex test^HL70389",
    ] {
        let capabilities = law::Capabilities {
            instructions: law::PeerInstructions {
                repeat: law::PeerRepeat {
                    manager_repeat: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let awos = law::Awos::new(
            b"awos-1",
            b"sample&Local",
            b"SER^^HL70487",
            b"P",
            b"tube",
            TEST.as_bytes(),
            law::WorkAction::New,
        )?
        .with_instructions(law::Instructions {
            repeat: Some(code),
            ..Default::default()
        });
        let output = law::work_order(
            &[awos],
            None,
            law::Address {
                application: b"M",
                facility: b"L",
                peer_application: b"A",
                peer_facility: b"L",
            },
            analyte_hl7::Response {
                control_id: law::control_id(b"priority")?,
                timestamp: b"20260101120000+0000",
            },
            capabilities,
            options(),
        )?;
        let tcd = output.segments().find(|s| s.id() == b"TCD").ok_or("TCD")?;
        assert_eq!(tcd.field(8).ok_or("repeat status")?.raw(), code);
    }
    Ok(())
}
