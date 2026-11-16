//! Tests law tf2b c1 component failures report precise locations; law tf2b c6 c7 c9 typed work fields preserve wire.
use super::*;

#[test]
fn law_tf2b_c1_component_failures_report_precise_locations()
-> Result<(), Box<dyn std::error::Error>> {
    for (body, field, component) in [
        ("ERR||OBX^x|100^^HL70357|E", 2, 2),
        ("ERR|||999^^HL70357|E", 3, 1),
        ("ERR|||100^^OTHER|E", 3, 3),
        ("ERR|||100^^HL70357|E|LOCAL^^OTHER", 5, 3),
    ] {
        let message = query(body)?;
        let segment = message
            .segments()
            .nth(1)
            .ok_or("missing synthetic segment")?;
        assert_eq!(
            analyte_hl7::law::validate_segment(segment),
            Err(Error::Component {
                segment: "ERR",
                field,
                component,
            })
        );
    }
    Ok(())
}
#[test]
fn law_tf2b_c6_c7_c9_typed_work_fields_preserve_wire() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::{Awos, Instructions, WorkAction, WorkCode, WorkFields, work_order};
    let fields = WorkFields::new(
        "sample-01",
        "99LAB",
        WorkCode {
            code: "SER",
            text: "Serum",
            system: "HL70487",
        },
        WorkCode {
            code: "test",
            text: "Test",
            system: "99LAB",
        },
        Some("R"),
    )?;
    let typed = fields.awos(b"awos-1", b"P", b"000tube", WorkAction::New)?;
    let encoded = Awos::new(
        b"awos-1",
        b"sample-01&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"000tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?
    .with_instructions(Instructions {
        priority: Some(b"R^^HL70485"),
        ..Instructions::default()
    });
    let render = |awos| {
        work_order(
            &[awos],
            None,
            address(),
            response_metadata().expect("synthetic metadata"),
            analyte_hl7::law::Capabilities {
                instructions: analyte_hl7::law::PeerInstructions {
                    priority: true,
                    ..work_capabilities().instructions
                },
                ..work_capabilities()
            },
            utf8_options(),
        )
    };
    assert_eq!(render(typed)?.raw(), render(encoded)?.raw());
    assert!(
        WorkFields::new(
            "sample^injected",
            "99LAB",
            WorkCode {
                code: "SER",
                text: "",
                system: "HL70487"
            },
            WorkCode {
                code: "test",
                text: "",
                system: "99LAB"
            },
            None
        )
        .is_err()
    );
    Ok(())
}
