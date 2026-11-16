//! Tests LAW specimen and container segments.
use super::*;

#[test]
fn law_tf2b_c11_sac_24_re_ce_units_identifier_system_predicate() -> TestResult {
    for value in ["", "mL^Milliliters^UCUM", "^Vendor unit"] {
        send_only("SAC", &[(24, value)], true)?;
    }
    for value in [
        "\"\"",
        "mL",
        "mL^^UCUM",
        "mL^Units",
        "mL^Units^OTHER",
        "^Units^UCUM",
        "mL^Units^UCUM~mL^Units^UCUM",
        "mL&bad^Units^UCUM",
    ] {
        send_only("SAC", &[(24, value)], false)?;
    }
    send_only(
        "SAC",
        &[(
            24,
            &format!("{}^{}^UCUM", "x".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    send_only(
        "SAC",
        &[(24, &format!("{}^Units^UCUM", "x".repeat(21)))],
        false,
    )?;
    send_only(
        "SAC",
        &[(24, &format!("mL^{}^UCUM", "x".repeat(200)))],
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_29_re_sn_exact_ratio_and_positive_denominator() -> TestResult {
    for value in [
        "",
        "^1^:^1",
        "^1^:^5.5",
        "^1^:^.5",
        "^1^:^+5",
        "^1^:^123456789012345",
    ] {
        send_only("SAC", &[(29, value)], true)?;
    }
    for value in [
        "\"\"",
        "<^1^:^5",
        "^2^:^5",
        "^1^/^5",
        "^1^:^",
        "^1^:^0",
        "^1^:^-1",
        "^1^:^NaN",
        "^1^:^1e3",
        "^1^:^1.2.3",
        "^1^:^1234567890123456",
        "^1^:^5~^1^:^5",
        "^1^:^5&bad",
    ] {
        send_only("SAC", &[(29, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_2_2_x_fields_composites_and_option_resolution() -> TestResult {
    for (id, supported) in [
        ("SPM", vec![1, 2, 3, 4, 7, 8, 9, 11, 13, 16, 17, 18, 27]),
        ("SAC", vec![3, 4, 9, 10, 11, 13, 14, 15, 21, 22, 24, 29]),
    ] {
        for number in 1..=35 {
            if supported.contains(&number) {
                continue;
            }
            check(
                id,
                &[(number, "ignored^data&too~here")],
                Profile::receive(),
                ControlDirection::Receive,
            )?;
            assert!(
                check(
                    id,
                    &[(number, "ignored")],
                    Profile::receive(),
                    ControlDirection::Send
                )
                .is_err()
            );
        }
    }
    for (id, field, value) in [
        ("SPM", 2, "id&Local&oid&ISO&ignored^filler"),
        ("SPM", 3, "id&Local^filler"),
        ("SPM", 4, "SER^^HL70487^ignored"),
        ("SPM", 11, "P^^HL70369^ignored"),
        ("SAC", 3, "tube^namespace"),
        ("SAC", 4, "parent^namespace"),
        ("SAC", 10, "rack^namespace"),
        ("SAC", 13, "tray^namespace"),
    ] {
        let mut changes = vec![(field, value)];
        if field == 10 && id == "SAC" {
            changes.push((11, "1"));
        }
        if field == 13 && id == "SAC" {
            changes.push((14, "1"));
        }
        check(id, &changes, Profile::receive(), ControlDirection::Receive)?;
        assert!(check(id, &changes, Profile::receive(), ControlDirection::Send).is_err());
    }
    let disabled = Profile::for_peer(Capabilities::default());
    for (id, field, value) in [
        ("SPM", 2, "id&Local"),
        ("SPM", 3, "id&Local"),
        ("SPM", 7, "id^^99LAB"),
        ("SPM", 8, "id^^99LAB"),
        ("SPM", 9, "id^^99LAB"),
        ("SPM", 16, "id^^99LAB"),
        ("SPM", 17, "20240101120000"),
        ("SPM", 18, "20240101120000"),
        ("SPM", 27, "id^^99LAB"),
        ("SAC", 9, "id^^HL70378"),
        ("SAC", 21, "1"),
        ("SAC", 22, "1"),
        ("SAC", 24, "^Units"),
        ("SAC", 29, "^1^:^5"),
    ] {
        check(id, &[(field, value)], disabled, ControlDirection::Receive)?;
        assert!(check(id, &[(field, value)], disabled, ControlDirection::Send).is_err());
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_composite_field_ceiling_including_empty_positions() -> TestResult {
    for (id, number, value, maximum) in [
        ("SPM", 2, "id&Local", 80),
        ("SPM", 3, "id&Local", 80),
        ("SPM", 4, "SER^^HL70487", 250),
        ("SPM", 7, "id^^HL70488", 250),
        ("SPM", 8, "id^^99LAB", 250),
        ("SPM", 9, "id^^99LAB", 250),
        ("SPM", 11, "P^^HL70369", 250),
        ("SPM", 16, "id^^HL70489", 250),
        ("SPM", 17, "20240101120000", 26),
        ("SPM", 18, "20240101120000", 26),
        ("SPM", 27, "id^^99LAB", 250),
        ("SAC", 3, "tube", 80),
        ("SAC", 4, "parent", 80),
        ("SAC", 9, "id^^HL70378", 250),
        ("SAC", 24, "mL^Units^UCUM", 250),
    ] {
        let exact = format!("{value}{}", "^".repeat(maximum - value.len()));
        check(
            id,
            &[(number, &exact)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
        assert!(
            check(
                id,
                &[(number, &format!("{exact}^"))],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_work_builder_validates_container_details_and_option_fields() -> TestResult {
    use analyte_hl7::Response;
    use law::{Address, Awos, ContainerDetails, SpecimenDetails, WorkAction};
    let awos = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^^HL70487",
        b"P",
        b"",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?;
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
            specimen: true,
            container: true,
            ..Capabilities::default().content
        },
        ..Capabilities::default()
    };
    // C.11: fail before a caller can reserve work using this render result.
    assert!(matches!(
        law::work_order(&[awos], None, address, response, capabilities, options()),
        Err(law::Error::Content {
            segment: "SAC",
            field: 4
        })
    ));
    let no_parent = ContainerDetails {
        carrier: Some(b"rack"),
        carrier_position: Some(b"1"),
        ..ContainerDetails::default()
    };
    assert!(matches!(
        law::work_order(
            &[awos.with_container_details(&no_parent)],
            None,
            address,
            response,
            capabilities,
            options()
        ),
        Err(law::Error::Content {
            segment: "SAC",
            field: 4
        })
    ));
    let specimen = SpecimenDetails {
        parents: Some(b"parent&Local"),
        collection_method: Some(b"id^^HL70488"),
        source_site: Some(b"id^^99LAB"),
        source_modifier: Some(b"id^^99LAB"),
        risk: Some(b"id^^HL70489"),
        collection_time: Some(b"20240101120000"),
        receipt_time: Some(b"20240101130000"),
        container_type: Some(b"id^^99LAB"),
    };
    let container = ContainerDetails {
        parent: Some(b"parent"),
        carrier_type: Some(b"id^^HL70378"),
        carrier: Some(b"rack"),
        carrier_position: Some(b"1^2"),
        tray: Some(b"tray"),
        tray_position: Some(b"1^2"),
        location: Some(b"location^^99LAB"),
        container_volume: Some(b"2.5"),
        specimen_volume: Some(b"1.2"),
        volume_units: Some(b"mL^Milliliters^UCUM"),
        dilution: Some(b"^1^:^5.5"),
    };
    let work = awos
        .with_specimen_details(&specimen)
        .with_container_details(&container);
    law::work_order(&[work], None, address, response, capabilities, options())?;
    validate_specimen_detail_cases(awos, address, response, capabilities, specimen)?;
    validate_container_detail_cases(awos, address, response, capabilities, container)?;
    Ok(())
}

#[test]
fn law_tf2b_3_28_4_1_2_1_negative_query_echoes_all_sac_identity_values() -> TestResult {
    use analyte_hl7::{Field, Response};
    use law::{Address, Query};
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
    for body in [
        "QPD|WOS^Synthetic^IHELAW|tag|000tube",
        "QPD|WOS_ALL^Synthetic^IHELAW|tag",
        "QPD|WOS_BY_RACK^Synthetic^IHELAW|tag||rack-42|7|||room^Bench^99LAB",
        "QPD|WOS_BY_TRAY^Synthetic^IHELAW|tag||||tray-73|3^8",
        "QPD|WOS_BY_ISOLATE^Synthetic^IHELAW|tag|child-41||||||parent-92",
        "QPD|WOS_INFORMATIONAL_RACK^Synthetic^IHELAW|tag|000tube|rack|2",
    ] {
        let wire = format!(
            "MSH|^~\\&|A|L|M|L|20260101120000+0000||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r"
        );
        let source = law::parse_message(wire.as_bytes(), options())?;
        let query = Query::new(&source)?;
        let output = law::negative_work(query, address, response, options())?;
        Profile::for_peer(Capabilities::default())
            .validate_message(&output, ControlDirection::Send)?;
        let qpd = source.segments().find(|s| s.id() == b"QPD").ok_or("QPD")?;
        let sac = output.segments().find(|s| s.id() == b"SAC").ok_or("SAC")?;
        for (target, parameter) in [(3, 3), (4, 9), (10, 4), (11, 5), (13, 6), (14, 7), (15, 8)] {
            let expected = if body.contains("WOS_ALL^") && target == 3 {
                b"\"\"".as_slice()
            } else {
                qpd.field(parameter).map_or(b"".as_slice(), Field::raw)
            };
            assert_eq!(
                sac.field(target).map_or(b"".as_slice(), Field::raw),
                expected,
                "{body}: SAC-{target}"
            );
        }
        let spm = output
            .segments()
            .find(|segment| segment.id() == b"SPM")
            .ok_or("fixture")?;
        assert_eq!(spm.field(4).map(Field::raw), Some(b"\"\"".as_slice()));
        let malformed = std::str::from_utf8(output.raw())?.replace("ORC|DC", "ORC|NW");
        let parsed = law::parse_message(malformed.as_bytes(), options())?;
        assert!(
            Profile::receive()
                .validate_message(&parsed, ControlDirection::Send)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_results_receive_preserves_x_and_rejects_bad_supported_identity() -> TestResult {
    let wire = "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|r1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||test^Synthetic^99LAB\rORC|SC||||IP\r";
    let good = law::parse_message(wire.as_bytes(), options())?;
    law::Results::new(&good)?;
    for (needle, value) in [
        ("sample&Local", "unqualified"),
        ("SER^^HL70487", "SER^^OTHER"),
        ("SAC|||tube", "SAC|||"),
        ("SPM|1|", "SPM|2|"),
    ] {
        let invalid = law::parse_message(wire.replace(needle, value).as_bytes(), options())?;
        let bytes = invalid.encode();
        assert!(law::Results::new(&invalid).is_err());
        assert_eq!(invalid.raw(), bytes);
    }
    let appended = wire.replace(
        "SAC|||tube",
        "SAC|||tube||||||ignored||||||||||||not-numeric",
    );
    let input = law::parse_message(appended.as_bytes(), options())?;
    law::Results::new(&input)?;
    assert_eq!(input.raw(), appended.as_bytes());
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_remaining_location_lengths_and_pool_receive_predicates() -> TestResult {
    for (field, value, size, position) in [
        (10, "rack", 80, 11),
        (13, "tray", 80, 14),
        (15, "location^^99LAB", 250, 11),
    ] {
        let exact = format!("{value}{}", "^".repeat(size - value.len()));
        let mut changes = if field == 15 {
            vec![(10, "rack"), (11, "1"), (field, exact.as_str())]
        } else {
            vec![(field, exact.as_str()), (position, "1")]
        };
        changes.extend([(3, ""), (4, "parent")]);
        check("SAC", &changes, Profile::receive(), ControlDirection::Send)?;
        let oversized = format!("{exact}^");
        let mut invalid = changes.clone();
        invalid.push((field, &oversized));
        assert!(check("SAC", &invalid, Profile::receive(), ControlDirection::Send).is_err());
    }
    let pool = Profile::receive().with_pooling_outside(true);
    for count in ["", "\"\"", "0", "1.5", "1000000", "2~2"] {
        assert!(
            check(
                "SPM",
                &[(11, "L^^HL70369"), (13, count)],
                pool,
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    check(
        "SPM",
        &[(11, "L^^HL70369"), (13, "999999")],
        pool,
        ControlDirection::Receive,
    )?;
    let no_patients = "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\rPID|||id||Family^^^^^^L\rSPM|1|sample&Local|parent&Local|SER^^HL70487|||||||L^^HL70369||1\r";
    let message = law::parse_message(no_patients.as_bytes(), options())?;
    assert!(
        pool.validate_message(&message, ControlDirection::Send)
            .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c12_role_predicates_require_the_standard_coding_system() -> TestResult {
    for code in ["P", "Q", "U", "L"] {
        both("SPM", &[(11, &format!("{code}^^99LAB"))], true)?;
    }
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(
            check(
                "SPM",
                &[(11, "U^^IHELAW"), (4, "\"\"")],
                Profile::receive(),
                direction
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_explicit_profile_cannot_enlarge_peer_send_capabilities() -> TestResult {
    use analyte_hl7::Response;
    use law::{Address, Awos, ContainerDetails, WorkAction};
    let details = ContainerDetails {
        container_volume: Some(b"1.5"),
        ..ContainerDetails::default()
    };
    let awos = Awos::new(
        b"a1",
        b"sample&Local",
        b"SER^^HL70487",
        b"P",
        b"tube",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?
    .with_container_details(&details);
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = Response {
        control_id: law::control_id(b"m1")?,
        timestamp: b"20260101120000+0000",
    };
    assert!(
        law::work_order_with_profile(
            &[awos],
            None,
            address,
            response,
            Capabilities::default(),
            options(),
            Profile::receive()
        )
        .is_err()
    );
    let supported = Capabilities {
        content: law::PeerContent {
            container: true,
            ..Capabilities::default().content
        },
        ..Capabilities::default()
    };
    law::work_order_with_profile(
        &[awos],
        None,
        address,
        response,
        supported,
        options(),
        Profile::receive(),
    )?;
    Ok(())
}
#[test]
fn law_tf2b_c11_am_carrier_tray_only_without_sac3_with_sac4() -> TestResult {
    // Original failing synthetic SAC bytes from the Gazelle export, C.11 M/X.
    let failing = law::parse_message(b"MSH|^~\\&|ANALYTE|ANALYTE-TEST|SYN-ANALYZER|ANALYTE-TEST|20260101120000+0000||OML^O33^OML_O33|SYN-OML1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE~LAW_SPECIMEN^IHE\rSPM|1|SYN-0001&ANALYTE-TEST||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||SYN-0001|||||||SYN-RACK1|1||||||||||||||||||\rORC|NW||||||||20260101120000||||||||||||\rOBR||SYN-AWOS1||SYN-TEST^Synthetic test^99SYN|||||||||||||||||||||||||||||||||||||||||||||\r", options())?;
    assert_eq!(
        Profile::for_peer(Capabilities {
            content: law::PeerContent {
                specimen: true,
                ..Capabilities::default().content
            },
            ..Capabilities::default()
        })
        .validate_message(&failing, ControlDirection::Send),
        Err(law::Error::AtSegment {
            occurrence: 1,
            source: Box::new(law::Error::Content {
                segment: "SAC",
                field: 10
            })
        })
    );
    for (identity, position) in [(10, 11), (13, 14)] {
        send_only("SAC", &[(identity, "rack"), (position, "1")], false)?;
        both(
            "SAC",
            &[(3, ""), (4, "parent"), (identity, "rack"), (position, "1")],
            true,
        )?;
        assert!(
            check(
                "SAC",
                &[(3, ""), (identity, "rack"), (position, "1")],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}
