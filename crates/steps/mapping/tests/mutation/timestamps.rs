//! Tests rejection of malformed inputs without losing evidence.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// HL7 2.5.1 §2.A.22
#[test]
fn partial_and_invalid_times_keep_evidence() -> TestResult {
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

// CLSI LIS02-A2 §6.14, §8.4.7, §9.10, §9.12, §9.13
#[test]
fn astm_timestamps_keep_role_and_original() -> TestResult {
    let raw=b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC||||20261115120100\rR|1|^^^SYN|1|||||F|20261114||20261115120200|20261115120300\rL|1|N\r";
    let messages = analyte_astm::parse(raw, analyte_astm::Limits::default())?;
    let mapped = map_astm(
        &analyte_mapping::astm::Settings {
            syntax: settings::astm(&config(ASTM_CONFIG)?).0,
            mappings: settings::astm(&config(ASTM_CONFIG)?).1,
        },
        messages.first().ok_or("message")?,
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

// Safety rule 2 (ADR 0003)
#[test]
fn conflicting_qc_classification_is_parked() -> TestResult {
    for (second, conflict) in [("patient", false), ("control", true)] {
        let definition=HL7_CONFIG.rewrite("qc = []",&format!("qc = [{{field={{group='order',record='OBR',field=3,component=1,repetition=1}},value='001',classification='patient'}},{{field={{group='order',record='OBR',field=3,component=1,repetition=1}},value='001',classification='{second}'}}]"));
        let mapped = qualified_hl7(
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

// ADR 0003 rule 2
#[test]
fn unknown_test_never_gets_resolved_identity() -> TestResult {
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

// CLSI LIS02-A2 §6.12
#[test]
fn qc_mode_is_kept_and_never_production() -> TestResult {
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

// Safety rule 5 (ADR 0003)
#[test]
fn hl7_note_text_and_position_are_kept() -> TestResult {
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

// Safety rule 7 (ADR 0003)
#[test]
fn identity_run_ceiling_is_inclusive() -> TestResult {
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

// IHE PaLM TF-2b §3.29.4, §B.3
#[test]
fn law_result_keeps_awos_and_observation_group() -> TestResult {
    let definition = include_str!("../../../../core/config/examples/law.toml").rewrite(
        "order_namespace = {fixed=\"LAB\"}",
        "order_namespace = {fixed=\"manager\"}",
    );
    let cfg = config(&definition)?;
    let raw = LAW_RESULT;
    let message =
        analyte_hl7::Message::parse(raw, effective_options(cfg.effective()).ok_or("options")?)?;
    let results = analyte_hl7::law::Results::new(&message)?;
    let mapped = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
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
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn astm_mapping_rechecks_limits_inclusively() -> TestResult {
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
                ASTM_CONFIG.rewrite(&format!("{key} = {old}"), &format!("{key} = {limit}"));
            let cfg = config(&definition)?;
            let mapped = map_astm(&settings::astm_mapping(&cfg), message, input());
            assert_eq!(mapped.is_ok(), accepted, "{key}={limit}: {mapped:?}");
            if !accepted {
                assert!(matches!(mapped, Err(analyte_mapping::Error::Limit { .. })));
            }
        }
    }
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn hl7_mapping_rechecks_limits_inclusively() -> TestResult {
    let raw = b"MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    for (key, old, ceiling) in [
        ("bytes", 1048576, raw.len()),
        ("records", 4096, message.segments().count()),
    ] {
        for (limit, accepted) in [(ceiling - 1, false), (ceiling, true), (ceiling + 1, true)] {
            let cfg = config(
                &HL7_CONFIG.rewrite(&format!("{key} = {old}"), &format!("{key} = {limit}")),
            )?;
            let mapped = map_hl7(&settings::hl7_mapping(&cfg), &message, input());
            assert_eq!(mapped.is_ok(), accepted, "{key}={limit}: {mapped:?}");
            if !accepted {
                assert!(matches!(mapped, Err(analyte_mapping::Error::Limit { .. })));
            }
        }
    }
    Ok(())
}

// ADR 0003
#[test]
fn unmapped_evidence_alone_requires_parking() -> TestResult {
    let mut output = astm("R|1|^^^UNKNOWN|1|||||F\r", "P")?;
    assert!(!output.unmapped.is_empty());
    // Output is public host-facing evidence; even a host that extracted findings
    // for display must not make unmapped observations eligible for delivery.
    output.findings.clear();
    output.results.clear();
    assert!(output.requires_parking());
    Ok(())
}

// IHE PaLM TF-2b §3.29.4.1.2.1
#[test]
fn status_only_ip_sc_ca_become_notices() -> TestResult {
    let cfg = config(include_str!("../../../../core/config/examples/law.toml"))?;
    let original = std::str::from_utf8(LAW_RESULT)?;
    let prefix = original.split("OBX|").next().ok_or("fixture")?;
    for status in ["IP", "SC", "CA"] {
        let wire = prefix.rewrite("ORC|SC||||CM", &format!("ORC|SC||||{status}"));
        let message = analyte_hl7::Message::parse(
            wire.as_bytes(),
            effective_options(cfg.effective()).ok_or("options")?,
        )?;
        let results = analyte_hl7::law::Results::new(&message)?;
        let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
        assert!(!output.requires_parking());
        assert!(output.results.is_empty());
        assert_eq!(output.status_notices.len(), 1);
        let notice = output.status_notices.first().ok_or("notice")?;
        assert_eq!(
            notice.execution,
            match status {
                "IP" => AwosExecution::InProgress,
                "SC" => AwosExecution::Scheduled,
                "CA" => AwosExecution::AnalyzerCancelled,
                _ => return Err("test status".into()),
            }
        );
        assert!(matches!(
            notice.estimate,
            analyte_mapping::law::AvailabilityEstimate::Absent
        ));
        assert_eq!(message.raw(), wire.as_bytes());
    }
    Ok(())
}

// IHE PaLM TF-2b §C.8, table C.8-9
#[test]
fn estimate_keeps_spelling_null_and_header_zone() -> TestResult {
    use analyte_mapping::law::AvailabilityEstimate;
    let cfg = config(include_str!("../../../../core/config/examples/law.toml"))?;
    let original = std::str::from_utf8(LAW_RESULT)?;
    let prefix = original.split("OBX|").next().ok_or("fixture")?;
    let before_orc = prefix.split("ORC|").next().ok_or("ORC")?;
    for estimate in ["", "\"\"", "20260101", "20260101123000"] {
        let wire = format!(
            "{before_orc}{}",
            segment("ORC", 27, &[(1, "SC"), (5, "IP"), (27, estimate)])?
        );
        let message = analyte_hl7::Message::parse(
            wire.as_bytes(),
            effective_options(cfg.effective()).ok_or("options")?,
        )?;
        let results = analyte_hl7::law::Results::new(&message)?;
        let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
        let notice = output.status_notices.first().ok_or("notice")?;
        match (&notice.estimate, estimate) {
            (AvailabilityEstimate::Absent, "") | (AvailabilityEstimate::Null, "\"\"") => {}
            (
                AvailabilityEstimate::Supplied {
                    original,
                    header_time,
                    offset_from_header,
                },
                supplied,
            ) => {
                assert_eq!(original.as_str(), supplied);
                assert!(header_time.as_str().ends_with("+0000"));
                assert_eq!(*offset_from_header, Some(UtcOffset::try_from(0)?));
            }
            _ => return Err("estimate distinction lost".into()),
        }
        assert!(!output.requires_parking());
        assert!(output.results.is_empty());
    }
    for estimate in [
        "20260101123000+0100",
        "20260101123000^S",
        "20260230",
        "20260101123000^",
    ] {
        let wire = format!(
            "{before_orc}{}",
            segment("ORC", 27, &[(1, "SC"), (5, "IP"), (27, estimate)])?
        );
        let message = analyte_hl7::Message::parse(
            wire.as_bytes(),
            effective_options(cfg.effective()).ok_or("options")?,
        )?;
        assert!(
            analyte_hl7::law::Results::new(&message).is_err(),
            "{estimate}"
        );
    }
    Ok(())
}

// IHE PaLM TF-2b §3.29.4.1.2.1
#[test]
fn status_groups_never_hide_result_groups() -> TestResult {
    let cfg = config(include_str!("../../../../core/config/examples/law.toml"))?;
    let original = std::str::from_utf8(LAW_RESULT)?;
    let order = original
        .split("OBR|")
        .nth(1)
        .ok_or("OBR")?
        .split("ORC|")
        .next()
        .ok_or("ORC")?;
    let wire = format!("{original}OBR|{order}ORC|SC||||IP\r");
    let message = analyte_hl7::Message::parse(
        wire.as_bytes(),
        effective_options(cfg.effective()).ok_or("options")?,
    )?;
    let results = analyte_hl7::law::Results::new(&message)?;
    assert_eq!(results.status_notices().len(), 1);
    assert_eq!(results.observations().len(), 1);
    let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
    assert_eq!(output.status_notices.len(), 1);
    assert_eq!(output.results.len(), 1);
    Ok(())
}

// IHE PaLM TF-2b §C.8, table C.8-9
#[test]
fn estimate_zone_from_msh7_and_ip_results_kept() -> TestResult {
    use analyte_mapping::law::AvailabilityEstimate;
    let cfg = config(include_str!("../../../../core/config/examples/law.toml"))?;
    let original = std::str::from_utf8(LAW_RESULT)?;
    let prefix = original.split("OBX|").next().ok_or("fixture")?;
    let before_orc = prefix
        .split("ORC|")
        .next()
        .ok_or("ORC")?
        .rewrite("+0000", "-0330");
    let wire = format!(
        "{before_orc}{}",
        segment("ORC", 27, &[(1, "SC"), (5, "IP"), (27, "20260101123000")])?
    );
    let message = analyte_hl7::Message::parse(
        wire.as_bytes(),
        effective_options(cfg.effective()).ok_or("options")?,
    )?;
    let results = analyte_hl7::law::Results::new(&message)?;
    let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
    let AvailabilityEstimate::Supplied {
        original,
        header_time,
        offset_from_header,
    } = &output.status_notices.first().ok_or("notice")?.estimate
    else {
        return Err("missing estimate".into());
    };
    assert_eq!(original.as_str(), "20260101123000");
    assert!(header_time.as_str().ends_with("-0330"));
    assert_eq!(*offset_from_header, Some(UtcOffset::try_from(-12600)?));
    let wire = std::str::from_utf8(LAW_RESULT)?.rewrite("ORC|SC||||CM", "ORC|SC||||IP");
    let message = analyte_hl7::Message::parse(
        wire.as_bytes(),
        effective_options(cfg.effective()).ok_or("options")?,
    )?;
    let results = analyte_hl7::law::Results::new(&message)?;
    assert!(results.status_notices().is_empty());
    let output = analyte_mapping::law::map(settings::law(&cfg), &results, input())?;
    assert!(output.status_notices.is_empty());
    assert_eq!(output.results.len(), 1);
    Ok(())
}
