//! Supplies synthetic law order test fixtures and scenarios.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection, Error, OrderControl, OrderStatus, Profile},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const TEST: &str = "test^Synthetic^99LAB";
fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn message(
    id: &str,
    direction: ControlDirection,
    changes: &[(usize, &str)],
) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = [""; 52];
    let defaults = match id {
        "ORC" => match direction {
            ControlDirection::Send => vec![(1, "NW"), (9, "20260101120000")],
            ControlDirection::Receive => vec![(1, "SC"), (5, "IP")],
        },
        "OBR" => vec![(2, "awos-1"), (4, TEST)],
        "TCD" => vec![(1, TEST)],
        "TQ1" => vec![(9, "R^^HL70485")],
        _ => return Err("fixture segment".into()),
    };
    for (number, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(number).ok_or("fixture field")? = value;
    }
    let wire = format!(
        "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\r{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    );
    Ok(law::parse_message(wire.as_bytes(), options())?)
}
fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(id, direction, changes)?;
    let original = message.encode();
    profile.validate_segment(message.segments().nth(1).ok_or("fixture")?, direction)?;
    assert_eq!(message.raw(), original);
    Ok(())
}
fn both(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert_eq!(
            check(id, changes, Profile::receive(), direction).is_ok(),
            valid,
            "{id} {changes:?} {direction:?}"
        );
    }
    Ok(())
}
fn manager(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    assert_eq!(
        check(id, changes, Profile::receive(), ControlDirection::Send).is_ok(),
        valid,
        "{id} {changes:?}"
    );
    check(id, changes, Profile::receive(), ControlDirection::Receive)
}

fn result_wire(detail: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = [""; 29];
    for (position, value) in [
        (1_usize, "1"),
        (2, "NM"),
        (3, TEST),
        (4, "1"),
        (5, "1.20"),
        (6, "mg/L^mg/L^UCUM"),
        (8, "N^Synthetic^HL70078"),
        (11, "F"),
        (16, "operator^Synthetic"),
        (18, "instrument^99LAB~module^99LAB"),
        (19, "20260101120000"),
        (29, "RSLT"),
    ] {
        *fields
            .get_mut(position.checked_sub(1).ok_or("fixture position")?)
            .ok_or("fixture")? = value;
    }
    Ok(format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|r1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||{TEST}\rORC|SC||||CM\rOBX|{}\r{detail}",
        fields.join("|")
    ))
}

fn validate_builder_repeat_permissions(
    awos: law::Awos<'_>,
    work: law::Awos<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: law::Capabilities,
) -> TestResult {
    use law::{Capabilities, Instructions, OrderDetails};
    let invalid_group = OrderDetails {
        group: Some(b"group&namespace"),
        ..OrderDetails::default()
    };
    assert!(
        law::work_order(
            &[work.with_order_details(&invalid_group)],
            None,
            address,
            response,
            capabilities,
            options()
        )
        .is_err()
    );
    let permission_caps = Capabilities {
        instructions: law::PeerInstructions {
            repeat: law::PeerRepeat {
                repeat_control: true,
                ..Capabilities::default().instructions.repeat
            },
            ..Capabilities::default().instructions
        },
        ..Capabilities::default()
    };
    assert!(law::work_order(&[awos], None, address, response, permission_caps, options()).is_err());
    let permissions = awos.with_instructions(Instructions {
        automatic_repeat: Some(false),
        reflex: Some(false),
        ..Instructions::default()
    });
    law::work_order(
        &[permissions],
        None,
        address,
        response,
        permission_caps,
        options(),
    )?;
    let repeat_caps = Capabilities {
        instructions: law::PeerInstructions {
            repeat: law::PeerRepeat {
                manager_repeat: true,
                ..Capabilities::default().instructions.repeat
            },
            ..Capabilities::default().instructions
        },
        ..Capabilities::default()
    };
    assert!(law::work_order(&[awos], None, address, response, repeat_caps, options()).is_err());
    law::work_order(
        &[awos.with_instructions(Instructions {
            repeat: Some(b"O^Synthetic^HL70389"),
            ..Instructions::default()
        })],
        None,
        address,
        response,
        repeat_caps,
        options(),
    )?;
    Ok(())
}

mod scenarios_1;

mod scenarios_2;

mod scenarios_3;
