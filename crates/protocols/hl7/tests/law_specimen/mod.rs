//! Supplies synthetic law specimen test fixtures and scenarios.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, Capabilities, ControlDirection, Profile},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn message(id: &str, changes: &[(usize, &str)]) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = [""; 36];
    let defaults = if id == "SPM" {
        vec![
            (1, "1"),
            (2, "sample&99LAB"),
            (4, "SER^^HL70487"),
            (11, "P^^HL70369"),
        ]
    } else {
        vec![(3, "tube")]
    };
    for (number, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(number).ok_or("fixture")? = value;
    }
    let bytes = format!(
        "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\r{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    );
    Ok(law::parse_message(bytes.as_bytes(), options())?)
}
fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(id, changes)?;
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
fn send_only(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    assert_eq!(
        check(id, changes, Profile::receive(), ControlDirection::Send).is_ok(),
        valid,
        "{id} {changes:?}"
    );
    check(id, changes, Profile::receive(), ControlDirection::Receive)
}

fn validate_carrier_axis_limits(identity: usize, position: usize) -> TestResult {
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "1234567890123456"),
        ],
        true,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "12345678901234567"),
        ],
        false,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "1^12345678901234567"),
        ],
        false,
    )?;
    assert!(
        check(
            "SAC",
            &[(3, ""), (4, "parent"), (identity, "rack")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(3, ""), (4, "parent"), (identity, "rack")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "SAC",
            &[(position, "1")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(position, "1")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    let exact = format!(
        "{}^{}^{}^{}^{}",
        "1".repeat(16),
        "2".repeat(16),
        "3".repeat(16),
        "4".repeat(16),
        "5".repeat(12)
    );
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, &exact),
        ],
        true,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, &format!("{exact}1")),
        ],
        false,
    )?;
    Ok(())
}

fn validate_specimen_detail_cases(
    awos: law::Awos<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
    specimen: law::SpecimenDetails<'_>,
) -> TestResult {
    use law::SpecimenDetails;
    for invalid in [
        SpecimenDetails {
            parents: Some(b"unqualified"),
            ..specimen
        },
        SpecimenDetails {
            source_modifier: Some(b"id^^99LAB~other^^99LAB"),
            ..specimen
        },
        SpecimenDetails {
            collection_time: Some(b"20230229120000"),
            ..specimen
        },
        SpecimenDetails {
            risk: Some(b"id^^OTHER"),
            ..specimen
        },
    ] {
        assert!(
            law::work_order(
                &[awos.with_specimen_details(&invalid)],
                None,
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    Ok(())
}

fn validate_container_detail_cases(
    awos: law::Awos<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
    container: law::ContainerDetails<'_>,
) -> TestResult {
    use law::ContainerDetails;
    for invalid in [
        ContainerDetails {
            carrier_position: None,
            ..container
        },
        ContainerDetails {
            tray_position: Some(b"1.5"),
            ..container
        },
        ContainerDetails {
            container_volume: Some(b"1e3"),
            ..container
        },
        ContainerDetails {
            volume_units: Some(b"mL^Units^OTHER"),
            ..container
        },
        ContainerDetails {
            dilution: Some(b"^1^:^0"),
            ..container
        },
    ] {
        assert!(
            law::work_order(
                &[awos.with_container_details(&invalid)],
                None,
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    // Carrier/parent identity is base information, not gated by LAW_CONTAINER.
    let base_details = ContainerDetails {
        parent: Some(b"parent"),
        carrier: Some(b"rack"),
        carrier_position: Some(b"1"),
        ..ContainerDetails::default()
    };
    law::work_order(
        &[awos.with_container_details(&base_details)],
        None,
        address,
        response,
        Capabilities::default(),
        options(),
    )?;
    assert!(
        law::work_order(
            &[awos.with_container_details(&container)],
            None,
            address,
            response,
            Capabilities::default(),
            options()
        )
        .is_err()
    );
    Ok(())
}

mod scenarios_1;

mod scenarios_2;
