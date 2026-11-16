//! Tests LAW patient and visit segments.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, AuthorityBinding, Capabilities, ControlDirection, Error, Profile},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}

fn message(id: &str, changes: &[(usize, &str)]) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = vec![""; 40];
    if id == "PID" {
        *fields.get_mut(3).ok_or("fixture")? = "000Ab";
        *fields.get_mut(5).ok_or("fixture")? = "Family^Given^^^^^L";
    } else {
        *fields.get_mut(2).ok_or("fixture")? = "I";
    }
    for (position, value) in changes {
        *fields.get_mut(*position).ok_or("fixture")? = value;
    }
    let bytes = format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\r{id}|{}\r",
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
    let segment = message.segments().nth(1).ok_or("fixture")?;
    profile.validate_segment(segment, direction)?;
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

#[test]
fn law_tf2b_c9_pid_3_5_r_singleton_null_when_not_applicable() -> TestResult {
    for number in [3, 5] {
        both("PID", &[], true)?;
        both("PID", &[(number, "\"\"")], true)?;
        for value in ["", "\"\"~\"\""] {
            both("PID", &[(number, value)], false)?;
        }
    }
    both("PID", &[(3, "first~second")], false)?;
    both("PID", &[(5, "Family^^^^^^L~Family^^^^^^L")], false)?;
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_3_cx_id_and_assigning_authority_lengths_types_predicate() -> TestResult {
    for value in [
        "000Ab",
        "000Ab^^^Local",
        "000Ab^^^&1.2.3&ISO",
        "000Ab^^^&2.25.1&ISO",
    ] {
        both("PID", &[(3, value)], true)?;
    }
    both("PID", &[(3, &"\u{e9}".repeat(50))], true)?;
    both("PID", &[(3, &"x".repeat(51))], false)?;
    for value in [
        "^^^Local",
        "id^^^&oid",
        "id^^^&oid&OTHER",
        "id^^^\"\"",
        "id&extra",
        "id^^^local&\"\"&ISO",
    ] {
        both("PID", &[(3, value)], false)?;
    }
    both("PID", &[(3, &format!("id^^^{}", "x".repeat(20)))], true)?;
    both("PID", &[(3, &format!("id^^^{}", "x".repeat(21)))], false)?;
    both(
        "PID",
        &[(
            3,
            &format!("id^^^&{}&ISO", format_args!("2.{}", "1".repeat(197))),
        )],
        true,
    )?;
    both(
        "PID",
        &[(
            3,
            &format!("id^^^&{}&ISO", format_args!("2.{}", "1".repeat(198))),
        )],
        false,
    )?;
    for kind in [
        "DNS", "GUID", "HCD", "HL7", "ISO", "L", "M", "N", "Random", "URI", "UUID", "x400", "x500",
    ] {
        both("PID", &[(3, &format!("id^^^&1.2.3&{kind}"))], kind == "ISO")?;
    }
    check(
        "PID",
        &[(3, "id^^^Local&&ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "PID",
            &[(3, "id^^^Local&&ignored")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_3_4_all_three_authority_names_require_a_trusted_binding() -> TestResult {
    let bindings = [AuthorityBinding::new(b"Local", b"1.2.3", b"ISO")?];
    let profile = Profile::receive().with_authorities(&bindings);
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        check("PID", &[(3, "id^^^Local&1.2.3&ISO")], profile, direction)?;
        assert!(check("PID", &[(3, "id^^^Local&1.2.4&ISO")], profile, direction).is_err());
        let unresolved = message("PID", &[(3, "id^^^Local&1.2.3&ISO")])?;
        let segment = unresolved.segments().nth(1).ok_or("fixture")?;
        assert_eq!(
            Profile::receive().validate_segment(segment, direction),
            Err(Error::Authority {
                segment: "PID",
                field: 3,
                component: 4
            })
        );
    }
    let conflicting = [
        AuthorityBinding::new(b"Local", b"1.2.3", b"ISO")?,
        AuthorityBinding::new(b"Local", b"1.2.4", b"ISO")?,
    ];
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(
            check(
                "PID",
                &[(3, "id^^^Local&1.2.3&ISO")],
                Profile::receive().with_authorities(&conflicting),
                direction
            )
            .is_err()
        );
    }
    assert!(AuthorityBinding::new(b"", b"1.2.3", b"ISO").is_err());
    assert!(AuthorityBinding::new(b"Local", b"1.2.3", b"BAD").is_err());
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_5_xpn_legal_name_components_hash_lengths_preserve_full_text() -> TestResult {
    both("PID", &[(5, "^^^^^^L")], true)?;
    both("PID", &[(5, "Family^Given^Middle^Jr^^^L")], true)?;
    for component in [1, 2, 3, 4] {
        let long = "\u{e9}".repeat(300);
        let mut parts = [""; 7];
        *parts.get_mut(6).ok_or("fixture")? = "L";
        *parts.get_mut(component - 1).ok_or("fixture")? = &long;
        both("PID", &[(5, &parts.join("^"))], true)?;
    }
    for value in [
        "Family",
        "Family^^^^^^A",
        "Family^^^^^^LL",
        "Family^^^^^^\"\"",
        "Family^Given&invalid^^^^^L",
        "Family^\"\"^^^^^L",
    ] {
        both("PID", &[(5, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_7_re_ts_date_or_seconds_no_zone() -> TestResult {
    for value in ["", "20240229", "20240229123059"] {
        both("PID", &[(7, value)], true)?;
    }
    for value in [
        "\"\"",
        "20230229",
        "20240001",
        "20240100",
        "20240101240000",
        "20240101126000",
        "20240101120060",
        "2024",
        "2024010112",
        "202401011230",
        "20240101123000+0000",
        "20240101~20240101",
        "20240101&extra",
    ] {
        both("PID", &[(7, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_8_re_sex_subset_singleton() -> TestResult {
    for value in ["", "F", "M", "U"] {
        both("PID", &[(8, value)], true)?;
    }
    for value in ["\"\"", "O", "N", "FF", "F~M", "F^x", "F&x"] {
        both("PID", &[(8, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_10_35_re_ce_cwe_components_systems_and_lengths() -> TestResult {
    for number in [10, 35] {
        for value in ["", "local^^99LAB", "local^Synthetic^99a1Z"] {
            both("PID", &[(number, value)], true)?;
        }
        both(
            "PID",
            &[(
                number,
                &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
            )],
            true,
        )?;
        for value in [
            "\"\"",
            "local",
            "^Text^99LAB",
            "local^^OTHER",
            "local^^99AB",
            "local^^99ABCD",
            "local^^99A!B",
            "local^^99LAB~local^^99LAB",
            "local&extra^^99LAB",
            "local^\"\"^99LAB",
        ] {
            both("PID", &[(number, value)], false)?;
        }
        both(
            "PID",
            &[(number, &format!("{}^^99LAB", "x".repeat(21)))],
            false,
        )?;
        both(
            "PID",
            &[(number, &format!("local^{}^99LAB", "x".repeat(200)))],
            false,
        )?;
    }
    both("PID", &[(10, "local^^HL70005")], true)?;
    both("PID", &[(35, "local^^HL70005")], false)?;
    Ok(())
}

#[test]
fn law_tf2b_c10_pv1_2_r_patient_class_subset() -> TestResult {
    for value in ["E", "I", "O", "P", "R", "B", "C", "N", "U"] {
        both("PV1", &[(2, value)], true)?;
    }
    for value in ["", "\"\"", "X", "II", "I~I", "I^x", "I&x"] {
        both("PV1", &[(2, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c10_pv1_3_re_pl_room_r_hash_16_preserved() -> TestResult {
    for value in ["", "^Room", "^12345678901234567"] {
        both("PV1", &[(3, value)], true)?;
    }
    both("PV1", &[(3, &format!("^{}", "\u{e9}".repeat(300)))], true)?;
    for value in ["\"\"", "Ward", "^", "^\"\"", "^room&extra", "^room~^room"] {
        both("PV1", &[(3, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c9_c10_2_2_x_fields_components_and_patient_option() -> TestResult {
    for (id, supported) in [("PID", vec![3, 5, 7, 8, 10, 35]), ("PV1", vec![2, 3])] {
        both(id, &[], true)?;
        for field in 1..=39 {
            if supported.contains(&field) {
                continue;
            }
            check(
                id,
                &[(field, "ignored&extra~too")],
                Profile::receive(),
                ControlDirection::Receive,
            )?;
            assert!(
                check(
                    id,
                    &[(field, "ignored")],
                    Profile::receive(),
                    ControlDirection::Send
                )
                .is_err()
            );
        }
        let disabled = Profile::for_peer(Capabilities::default());
        check(id, &[], disabled, ControlDirection::Receive)?;
        assert!(check(id, &[], disabled, ControlDirection::Send).is_err());
    }
    for (id, field, value) in [
        ("PID", 3, "id^ignored^ignored^^ignored"),
        ("PID", 5, "Family&ignored^Given^^^ignored^ignored^L^ignored"),
        ("PID", 7, "20240101^ignored"),
        ("PID", 10, "local^^99LAB^ignored"),
        ("PID", 35, "local^^99LAB^ignored"),
        ("PV1", 3, "Ward^Room^Bed"),
    ] {
        check(
            id,
            &[(field, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        assert!(
            check(
                id,
                &[(field, value)],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c9_c10_builder_and_receive_validate_profile() -> TestResult {
    use analyte_hl7::Response;
    use law::{Address, Awos, PatientInfo, Visit, WorkAction};
    let awos = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^Synthetic^HL70487",
        b"P",
        b"tube",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?;
    let patient = PatientInfo {
        notes: &[],
        identifiers: b"id",
        name: b"Family^Given^^^^^L",
        birth: Some(b"20240229"),
        sex: Some(b"U"),
        race: None,
        taxonomy: None,
        visit: Some(Visit {
            class: b"I",
            location: Some(b"^Room"),
        }),
    };
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = Response {
        control_id: law::control_id(b"out-1")?,
        timestamp: b"20260101120000+0000",
    };
    let capabilities = Capabilities {
        content: law::PeerContent {
            patient: true,
            specimen: true,
            ..Capabilities::default().content
        },
        ..Capabilities::default()
    };
    law::work_order(
        &[awos],
        Some(patient),
        address,
        response,
        capabilities,
        options(),
    )?;
    // PaLM TF-2b 2.2.4.5/C.9/C.10: supplied demographics survive rendering.
    let output = law::work_order(
        &[awos],
        Some(patient),
        address,
        response,
        capabilities,
        options(),
    )?;
    for (id, fields) in [
        (
            b"PID".as_slice(),
            vec![
                (3, b"id".as_slice()),
                (5, b"Family^Given^^^^^L"),
                (7, b"20240229"),
                (8, b"U"),
                (10, b""),
                (35, b""),
            ],
        ),
        (b"PV1".as_slice(), vec![(2, b"I".as_slice()), (3, b"^Room")]),
    ] {
        let segment = output
            .segments()
            .find(|s| s.id() == id)
            .ok_or("demographic segment")?;
        for (number, expected) in fields {
            assert_eq!(
                segment
                    .field(number)
                    .map_or(b"".as_slice(), analyte_hl7::Field::raw),
                expected,
                "{id:?}-{number}"
            );
        }
    }
    let absent = law::work_order(&[awos], None, address, response, capabilities, options())?;
    assert!(!absent.segments().any(|s| matches!(s.id(), b"PID" | b"PV1")));
    validate_patient_builder_cases(awos, patient, address, response, capabilities)?;
    Ok(())
}

#[test]
fn law_tf2b_c9_pid_3_cx_hd_aggregate_limits_and_nested_x() -> TestResult {
    let namespace = "n".repeat(20);
    let uid = format!("2.{}", "1".repeat(197));
    let bindings = [AuthorityBinding::new(
        namespace.as_bytes(),
        uid.as_bytes(),
        b"ISO",
    )?];
    let profile = Profile::receive().with_authorities(&bindings);
    let exact = format!("{}^^^{namespace}&{uid}&ISO", "i".repeat(50));
    check("PID", &[(3, &exact)], profile, ControlDirection::Send)?;
    assert!(
        check(
            "PID",
            &[(3, &format!("i{exact}"))],
            profile,
            ControlDirection::Send
        )
        .is_err()
    );
    for value in [
        "id^^^Local&&&ignored",
        "id^^^^ignored",
        "id^^^&1.2.3&ISO&ignored",
    ] {
        check("PID", &[(3, value)], profile, ControlDirection::Receive)?;
        assert!(check("PID", &[(3, value)], profile, ControlDirection::Send).is_err());
    }
    Ok(())
}

fn validate_patient_builder_cases(
    awos: law::Awos<'_>,
    patient: law::PatientInfo<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
) -> TestResult {
    use law::{PatientInfo, Visit};
    for invalid in [
        PatientInfo {
            birth: Some(b"20230229"),
            ..patient
        },
        PatientInfo {
            sex: Some(b"O"),
            ..patient
        },
        PatientInfo {
            visit: Some(Visit {
                class: b"I",
                location: Some(b"Ward^Room^Bed"),
            }),
            ..patient
        },
    ] {
        assert!(
            law::work_order(
                &[awos],
                Some(invalid),
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    let bindings = [AuthorityBinding::new(b"Local", b"1.2.3", b"ISO")?];
    let patient = PatientInfo {
        identifiers: b"id^^^Local&1.2.3&ISO",
        ..patient
    };
    assert!(
        law::work_order(
            &[awos],
            Some(patient),
            address,
            response,
            capabilities,
            options()
        )
        .is_err()
    );
    let profile = Profile::for_peer(capabilities).with_authorities(&bindings);
    law::work_order_with_profile(
        &[awos],
        Some(patient),
        address,
        response,
        capabilities,
        options(),
        profile,
    )?;
    // LAB-29 receives the same demographics without silently overwriting app data.
    let source = "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|r1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rPID|||id^^^Local&1.2.3&ISO||Family^^^^^^L\rSPM|1|sample&99LAB||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||test^Synthetic^99LAB\rORC|SC||||IP\r";
    let incoming = law::parse_message(source.as_bytes(), options())?;
    assert!(law::Results::new(&incoming).is_err());
    law::Results::with_profile(&incoming, profile)?;
    let bad = law::parse_message(
        source.replace("Family^^^^^^L", "Family^^^^^^A").as_bytes(),
        options(),
    )?;
    assert!(law::Results::with_profile(&bad, profile).is_err());
    Ok(())
}
