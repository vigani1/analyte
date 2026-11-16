//! Tests rejection of malformed inputs without losing evidence.
use super::*;

#[test]
fn hl7_251_2a22_partial_times_and_invalid_times_preserve_evidence_without_guessing() -> TestResult {
    for (raw, precision) in [
        ("2026", TimePrecision::Year),
        ("202611", TimePrecision::Month),
        ("20261115", TimePrecision::Day),
        ("2026111512", TimePrecision::Hour),
        ("202611151230", TimePrecision::Minute),
        ("20261115123000", TimePrecision::Second),
        ("20261115123000.1", TimePrecision::FractionalSecond),
        ("20261115123000.1234", TimePrecision::FractionalSecond),
        ("20261115123000.", TimePrecision::Unknown),
        ("20261115123000.12345", TimePrecision::Unknown),
        ("20261115123000.a", TimePrecision::Unknown),
        ("00001115123000", TimePrecision::Unknown),
        ("20261315123000", TimePrecision::Unknown),
        ("20261131123000", TimePrecision::Unknown),
        ("20261115123000+0160", TimePrecision::Unknown),
        ("20261115123000+2400", TimePrecision::Unknown),
    ] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||{raw}\r"),
            HL7_CONFIG,
        )?;
        let time = first(&mapped)?
            .times
            .as_slice()
            .iter()
            .find(|t| t.role.as_str() == "OBX.14")
            .ok_or("time")?;
        assert_eq!(time.time.original.as_str(), raw);
        assert_eq!(time.time.precision, precision, "{raw}");
        assert_eq!(
            time.time.parsed.is_some(),
            matches!(
                precision,
                TimePrecision::Second | TimePrecision::FractionalSecond
            ),
            "{raw}"
        );
        if precision == TimePrecision::FractionalSecond {
            assert!(time.time.parsed.ok_or("parsed")?.subsec_nanosecond() > 0);
        }
        assert_eq!(
            mapped
                .findings
                .iter()
                .any(|f| f.problem == Problem::InvalidTime),
            precision == TimePrecision::Unknown,
            "{raw}"
        );
    }
    for (raw, offset) in [
        ("20261115123000+0130", 5400),
        ("20261115123000-0500", -18000),
    ] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||{raw}\r"),
            HL7_CONFIG,
        )?;
        let time = first(&mapped)?
            .times
            .as_slice()
            .iter()
            .find(|t| t.role.as_str() == "OBX.14")
            .ok_or("time")?;
        assert_eq!(time.time.offset.map(i32::from), Some(offset));
    }
    Ok(())
}

#[test]
fn lis02_6_14_8_4_7_9_10_9_12_9_13_timestamps_keep_role_and_original() -> TestResult {
    let raw=b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC||||20261115120100\rR|1|^^^SYN|1|||||F|20261114||20261115120200|20261115120300\rL|1|N\r";
    let messages = analyte_astm::parse(raw, analyte_astm::Limits::default())?;
    let mapped = map_astm(
        messages.first().ok_or("message")?,
        &config(ASTM_CONFIG)?,
        input(),
    )?;
    assert_eq!(
        first(&mapped)?
            .times
            .as_slice()
            .iter()
            .map(|t| (t.role.as_str(), t.time.original.as_str()))
            .collect::<Vec<_>>(),
        [
            ("H.14", "20261115120000"),
            ("O.7", "20261115120100"),
            ("R.10", "20261114"),
            ("R.12", "20261115120200"),
            ("R.13", "20261115120300")
        ]
    );
    Ok(())
}

#[test]
fn rule2_conflicting_explicit_qc_classification_is_unknown_and_parked() -> TestResult {
    for (second, conflict) in [("patient", false), ("control", true)] {
        let definition=HL7_CONFIG.replace("qc = []",&format!("qc = [{{field={{group='order',record='OBR',field=3,component=1,repetition=1}},value='001',classification='patient'}},{{field={{group='order',record='OBR',field=3,component=1,repetition=1}},value='001',classification='{second}'}}]"));
        let mapped = hl7(
            "OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
            &definition,
        )?;
        assert_eq!(
            first(&mapped)?.qc_class,
            if conflict {
                QcClass::Unknown
            } else {
                QcClass::Patient
            }
        );
        assert_eq!(
            mapped
                .findings
                .iter()
                .any(|f| f.problem == Problem::ConflictingClassification),
            conflict
        );
        assert_eq!(mapped.requires_parking(), conflict);
    }
    Ok(())
}

#[test]
fn adr0003_rule2_unknown_test_with_source_namespace_never_gets_resolved_identity() -> TestResult {
    let mapped = hl7(
        "OBR|1||001\rOBX|1|NM|UNKNOWN^^synthetic||1||||||F\r",
        HL7_CONFIG,
    )?;
    assert!(mapped.requires_parking());
    assert!(matches!(
        mapped.results.first().ok_or("result")?.content.identity,
        IdentityStatus::Unresolved(_)
    ));
    assert!(
        mapped
            .findings
            .iter()
            .any(|f| f.problem == Problem::UnknownTest)
    );
    Ok(())
}

#[test]
fn lis02_6_12_quality_control_mode_is_retained_and_never_deliverable_as_production() -> TestResult {
    let mapped = astm("R|1|^^^SYN|1|||||F\r", "Q")?;
    assert_eq!(
        mapped.results.first().ok_or("result")?.content.processing,
        ProcessingMode::QualityControl
    );
    assert!(
        !mapped
            .results
            .first()
            .ok_or("result")?
            .content
            .may_deliver_to_production()
    );
    Ok(())
}

#[test]
fn rule5_hl7_note_text_and_field_position_are_retained() -> TestResult {
    let mapped = hl7(
        "OBR|1||001\rNTE|1||order note\rOBX|1|NM|SYN^^synthetic||1||||||F\rNTE|1||result note\r",
        HL7_CONFIG,
    )?;
    assert_eq!(
        mapped
            .comments
            .iter()
            .map(|n| (n.position.field, n.text.as_str()))
            .collect::<Vec<_>>(),
        [(3, "order note"), (3, "result note")]
    );
    Ok(())
}

#[test]
fn rule7_identity_run_accepts_exact_source_text_ceiling_and_rejects_larger() -> TestResult {
    for (length, accepted) in [(16377, true), (16378, true), (16379, false)] {
        let value = "X".repeat(length);
        let mapped = analyte_mapping::identity_run(&[&value]);
        assert_eq!(mapped.is_ok(), accepted);
        if accepted {
            assert_eq!(mapped?.as_str(), format!("{length}:{value}"));
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_3_29_4_b3_mapping_preserves_awos_og_and_refuses_generic_entry() -> TestResult {
    let definition = include_str!("../../../../core/config/examples/law.toml").replace("order_namespace = {field={group=\"order\",record=\"OBR\",field=2,component=2,repetition=1}}", "order_namespace = {fixed=\"manager\"}");
    let cfg = config(&definition)?;
    let raw = LAW_RESULT;
    let message =
        analyte_hl7::Message::parse(raw, cfg.effective().hl7_options().ok_or("options")?)?;
    let results = analyte_hl7::law::Results::new(&message)?;
    assert!(matches!(
        map_hl7(&message, &cfg, input()),
        Err(analyte_mapping::Error::Syntax)
    ));
    let mapped = analyte_mapping::map_law(&results, &cfg, input())?;
    assert!(!mapped.requires_parking(), "{mapped:?}");
    assert_eq!(mapped.results.len(), 1);
    let content = &mapped.results.first().ok_or("result")?.content;
    let IdentityStatus::Resolved(id) = &content.identity else {
        return Err("unresolved LAW identity".into());
    };
    assert_eq!(id.observation.order.value.as_str(), "awos-1");
    assert_eq!(id.observation.sub_id.as_str(), "1^1^1");
    assert_eq!(
        content.sample.as_ref().ok_or("sample")?.id.as_str(),
        "000tube"
    );
    assert_eq!(
        first(&mapped)?
            .values
            .as_slice()
            .first()
            .ok_or("value")?
            .original
            .as_str(),
        "1"
    );
    assert!(matches!(
        analyte_mapping::map_law(&results, &config(HL7_CONFIG)?, input()),
        Err(analyte_mapping::Error::Syntax)
    ));
    Ok(())
}

#[test]
fn rule7_astm_mapping_rechecks_independent_limits_inclusively() -> TestResult {
    let raw =
        b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^SYN|1|||||F\rL|1|N\r";
    let parsed = analyte_astm::parse(raw, analyte_astm::Limits::default())?;
    let message = parsed.first().ok_or("message")?;
    let max_record = message
        .records()
        .iter()
        .map(|r| r.raw().len())
        .max()
        .ok_or("record")?;
    for (key, old, ceiling) in [
        ("bytes", 1048576, raw.len()),
        ("records", 4096, message.records().len()),
        ("record_bytes", 65536, max_record),
    ] {
        for (limit, accepted) in [(ceiling - 1, false), (ceiling, true), (ceiling + 1, true)] {
            let definition =
                ASTM_CONFIG.replace(&format!("{key} = {old}"), &format!("{key} = {limit}"));
            let cfg = config(&definition)?;
            let mapped = map_astm(message, &cfg, input());
            assert_eq!(mapped.is_ok(), accepted, "{key}={limit}: {mapped:?}");
            if !accepted {
                assert!(matches!(mapped, Err(analyte_mapping::Error::Limit { .. })));
            }
        }
    }
    Ok(())
}

#[test]
fn rule7_hl7_mapping_rechecks_independent_limits_inclusively() -> TestResult {
    let raw = b"MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    for (key, old, ceiling) in [
        ("bytes", 1048576, raw.len()),
        ("records", 4096, message.segments().count()),
    ] {
        for (limit, accepted) in [(ceiling - 1, false), (ceiling, true), (ceiling + 1, true)] {
            let cfg = config(
                &HL7_CONFIG.replace(&format!("{key} = {old}"), &format!("{key} = {limit}")),
            )?;
            let mapped = map_hl7(&message, &cfg, input());
            assert_eq!(mapped.is_ok(), accepted, "{key}={limit}: {mapped:?}");
            if !accepted {
                assert!(matches!(mapped, Err(analyte_mapping::Error::Limit { .. })));
            }
        }
    }
    Ok(())
}

#[test]
fn adr0003_unmapped_evidence_alone_requires_parking() -> TestResult {
    let mut output = astm("R|1|^^^UNKNOWN|1|||||F\r", "P")?;
    assert!(!output.unmapped.is_empty());
    // Output is public host-facing evidence; even a host that extracted findings
    // for display must not make unmapped observations eligible for delivery.
    output.findings.clear();
    output.results.clear();
    assert!(output.requires_parking());
    Ok(())
}
