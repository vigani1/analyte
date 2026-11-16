//! Host bounds are from clinical rule 7; protocol assertions cite separately read sources.
use super::*;

#[test]
fn rule2_uninterpreted_numeric_comparator_never_becomes_equality() -> TestResult {
    for raw in ["??^1", "<>^1"] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|SN|SYN^^synthetic||{raw}||||||F\r"),
            HL7_CONFIG,
        )?;
        let value = first(&mapped)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), raw);
        assert!(value.parsed.is_unsupported());
        assert!(mapped.requires_parking());
    }
    Ok(())
}

#[test]
fn rule2_configured_coded_kind_refuses_empty_or_numeric_text_without_guessing() -> TestResult {
    let definition = HL7_CONFIG.replace(
        "app_system=\"demo\"",
        "app_system=\"demo\",value_kind=\"coded\"",
    );
    for (raw, mismatch) in [("positive", false), ("", true), ("123", true), ("<1", true)] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|ST|SYN^^synthetic||{raw}||||||F\r"),
            &definition,
        )?;
        assert_eq!(
            mapped
                .findings
                .iter()
                .any(|f| matches!(f.problem, Problem::ValueKindMismatch { .. })),
            mismatch,
            "{raw}"
        );
        assert_eq!(mapped.requires_parking(), mismatch, "{raw}");
    }
    Ok(())
}

#[test]
fn hl7_251_7_4_2_2_status_only_and_missing_populated_value_type_stay_distinct() -> TestResult {
    let raw=b"MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1||SYN^^synthetic||populated||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    assert!(matches!(
        map_hl7(&message, &config(HL7_CONFIG)?, input()),
        Err(analyte_mapping::Error::Structure {
            position: analyte_mapping::Position {
                record: 3,
                field: 2
            },
            ..
        })
    ));
    let empty = hl7("OBR|1||001\rOBX|1||SYN^^synthetic||||||||X\r", HL7_CONFIG)?;
    assert_eq!(first(&empty)?.status, ResultStatus::NotPerformed);
    assert_eq!(
        first(&empty)?
            .values
            .as_slice()
            .first()
            .ok_or("value")?
            .original
            .as_str(),
        ""
    );
    assert!(
        hl7(
            "OBR|1||001\rOBX|1||SYN^^synthetic||populated||||||F\r",
            HL7_CONFIG
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn rule2_operator_reason_distinguishes_uninterpreted_formatting_from_escape_agreement() -> TestResult
{
    for (raw, word) in [(r"a\H\b\N\c", "formatting"), (r"a\Zlocal\b", "escape")] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|TX|SYN^^synthetic||{raw}||||||F\r"),
            HL7_CONFIG,
        )?;
        let ValueInterpretation::Unsupported { reason, .. } = &first(&mapped)?
            .values
            .as_slice()
            .first()
            .ok_or("value")?
            .parsed
        else {
            return Err("unsupported evidence expected".into());
        };
        assert!(reason.as_str().contains(word));
        assert!(mapped.requires_parking());
    }
    Ok(())
}

#[test]
fn hl7_251_2a70_supported_numeric_comparators_and_ranges_keep_components() -> TestResult {
    for (code, expected) in [
        ("<", Comparator::Less),
        ("<=", Comparator::LessOrEqual),
        (">", Comparator::Greater),
        (">=", Comparator::GreaterOrEqual),
    ] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|SN|SYN^^synthetic||{code}^1||||||F\r"),
            HL7_CONFIG,
        )?;
        let ValueInterpretation::StructuredNumeric(value) = &first(&mapped)?
            .values
            .as_slice()
            .first()
            .ok_or("value")?
            .parsed
        else {
            return Err("SN".into());
        };
        assert_eq!(value.comparator, Some(expected));
        assert_eq!(value.first.as_ref().map(Decimal::as_str), Some("1"));
    }
    for sep in [":", "-", "+", "/"] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|SN|SYN^^synthetic||^1^{sep}^2||||||F\r"),
            HL7_CONFIG,
        )?;
        let ValueInterpretation::StructuredNumeric(value) = &first(&mapped)?
            .values
            .as_slice()
            .first()
            .ok_or("value")?
            .parsed
        else {
            return Err("SN".into());
        };
        assert_eq!(value.separator.as_str(), sep);
        assert_eq!(value.second.as_ref().map(Decimal::as_str), Some("2"));
    }
    Ok(())
}

#[test]
fn adr0003_coded_values_without_unambiguous_code_and_namespace_are_parked() -> TestResult {
    for raw in [
        "^Label^system",
        "code^Label",
        "\"\"^Label^system",
        "code^Label^\"\"",
    ] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|CE|SYN^^synthetic||{raw}||||||F\r"),
            HL7_CONFIG,
        )?;
        assert!(mapped.requires_parking());
        assert!(
            first(&mapped)?
                .values
                .as_slice()
                .first()
                .ok_or("value")?
                .parsed
                .is_unsupported()
        );
    }
    Ok(())
}

#[test]
fn rule5_numeric_offset_evidence_keeps_reported_components_without_inferring_zone() -> TestResult {
    // These are arithmetic evidence boundaries, not claims that these offsets
    // identify a legally defined timezone. No timezone is inferred or converted.
    for (suffix, offset) in [("+2300", 82800), ("+0159", 7140), ("-2300", -82800)] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000{suffix}\r"),
            HL7_CONFIG,
        )?;
        let time = first(&mapped)?
            .times
            .as_slice()
            .iter()
            .find(|t| t.role.as_str() == "OBX.14")
            .ok_or("time")?;
        assert_eq!(
            time.time.original.as_str(),
            format!("20261115123000{suffix}")
        );
        assert_eq!(time.time.offset.map(i32::from), Some(offset));
    }
    Ok(())
}

#[test]
fn hl7_251_ch7_oru_specimen_observations_keep_their_specimen_owner() -> TestResult {
    let mapped = hl7(
        "OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\rSPM|1\rOBX|1|NM|SYN^^synthetic||2||||||F\rOBR|2||002\rOBX|1|NM|SYN^^synthetic||3||||||F\r",
        HL7_CONFIG,
    )?;
    assert_eq!(mapped.results.len(), 3);
    let next_order = &mapped.results.get(2).ok_or("next order")?.content;
    assert_eq!(
        next_order.sample.as_ref().ok_or("sample")?.id.as_str(),
        "002"
    );
    assert!(
        !next_order
            .parts
            .as_slice()
            .first()
            .ok_or("part")?
            .source_path
            .as_slice()
            .iter()
            .any(|n| n.kind.as_str() == "SPM")
    );
    let part = mapped
        .results
        .get(1)
        .ok_or("specimen result")?
        .content
        .parts
        .as_slice()
        .first()
        .ok_or("part")?;
    assert!(
        part.source_path
            .as_slice()
            .iter()
            .any(|n| n.kind.as_str() == "SPM")
    );
    assert_eq!(
        part.values
            .as_slice()
            .first()
            .ok_or("value")?
            .original
            .as_str(),
        "2"
    );
    Ok(())
}

#[test]
fn hl7_251_ch7_oul_test_detail_and_notes_belong_to_observation_group() -> TestResult {
    let cfg = config(HL7_CONFIG)?;
    let header = "MSH|^~\\&|SYN||||202611151200||OUL^R22^OUL_R22|MSG|P|2.5.1\rSPM|1\rOBR|1||001\r";
    let raw = format!(
        "{header}OBX|1|NM|SYN^^synthetic||1||||||F\rTCD|SYN^^synthetic\rSID|lot\rNTE|1||observation note\r"
    );
    let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
    let mapped = map_hl7(&message, &cfg, input())?;
    assert_eq!(
        first(&mapped)?
            .comments
            .as_slice()
            .iter()
            .map(SourceText::as_str)
            .collect::<Vec<_>>(),
        ["observation note"]
    );
    assert_eq!(
        mapped
            .comments
            .first()
            .ok_or("comment")?
            .parent_path
            .as_slice()
            .last()
            .ok_or("parent")?
            .kind
            .as_str(),
        "OBX"
    );
    for detail in ["TCD|SYN^^synthetic\r", "SID|lot\r"] {
        let raw = format!("{header}{detail}OBX|1|NM|SYN^^synthetic||1||||||F\r");
        let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
        assert!(map_hl7(&message, &cfg, input()).is_err());
    }
    Ok(())
}

#[test]
fn rule2_hl7_mapping_refuses_unqualified_structure_without_losing_original() -> TestResult {
    let cfg = config(&HL7_CONFIG.replace("edition = \"v251\"", "edition = \"v231\""))?;
    let raw=b"MSH|^~\\&|SYN||||202611151200||OUL^R22|MSG|P|2.3.1\rSPM|1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    assert!(map_hl7(&message, &cfg, input()).is_err());
    assert_eq!(message.encode(), raw);
    let raw=b"MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rSAC|||container-without-specimen\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    assert!(map_hl7(&message, &config(HL7_CONFIG)?, input()).is_err());
    assert_eq!(message.encode(), raw);
    Ok(())
}

#[test]
fn rule3_hl7_visit_context_does_not_discard_the_result() -> TestResult {
    let mapped = hl7(
        "PID|1\rPV1|1|O\rPV2\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
        HL7_CONFIG,
    )?;
    assert_eq!(mapped.results.len(), 1);
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

#[test]
fn rule2_missing_group_relative_sample_reports_its_actual_owner_position() -> TestResult {
    let raw=b"MSH|^~\\&|SYN||||202611151200||OUL^R22^OUL_R22|MSG|P|2.5.1\rSPM|1\rOBR|1\rORC|SC||||IP\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    let mapped = map_hl7(&message, &config(HL7_CONFIG)?, input())?;
    let finding = mapped
        .findings
        .iter()
        .find(|f| f.problem == Problem::MissingSample)
        .ok_or("missing-sample finding")?;
    assert_eq!(
        finding.position,
        analyte_mapping::Position {
            record: 3,
            field: 3
        }
    );
    assert!(mapped.requires_parking());
    assert!(
        mapped
            .results
            .first()
            .ok_or("result")?
            .content
            .sample
            .is_none()
    );
    Ok(())
}

#[test]
fn rule7_value_source_text_bound_is_inclusive_in_both_syntaxes() -> TestResult {
    for length in [16383, 16384, 16385] {
        let value = "X".repeat(length);
        let hl7_result = hl7(
            &format!("OBR|1||001\rOBX|1|TX|SYN^^synthetic||{value}||||||F\r"),
            HL7_CONFIG,
        );
        let astm_result = astm(&format!("R|1|^^^SYN|{value}|||||F\r"), "P");
        for result in [hl7_result, astm_result] {
            assert_eq!(
                result.is_ok(),
                length <= 16384,
                "length={length}: {result:?}"
            );
            if let Ok(mapped) = result {
                assert_eq!(
                    first(&mapped)?
                        .values
                        .as_slice()
                        .first()
                        .ok_or("value")?
                        .original
                        .as_str(),
                    value
                );
            }
        }
    }
    Ok(())
}

fn segment(name: &str, count: usize, values: &[(usize, &str)]) -> Result<String, &'static str> {
    let mut fields = vec![""; count];
    for (field, value) in values {
        let slot = field
            .checked_sub(1)
            .and_then(|index| fields.get_mut(index))
            .ok_or("invalid synthetic field position")?;
        *slot = value;
    }
    Ok(format!("{name}|{}\r", fields.join("|")))
}

#[test]
fn rule3_hl7_source_path_keeps_statuses_sequences_and_specimen_times() -> TestResult {
    let cfg = config(HL7_CONFIG)?;
    let mut raw = "MSH|^~\\&|SYN||||202611151200||OUL^R22^OUL_R22|MSG|P|2.5.1\r".to_owned();
    raw.push_str(&segment(
        "SPM",
        19,
        &[
            (1, "1"),
            (17, "20261115110000"),
            (18, "20261115111000"),
            (19, "20261116110000"),
        ],
    )?);
    raw.push_str(&segment(
        "OBR",
        25,
        &[(1, "1"), (3, "001"), (7, "20261115110000"), (25, "F")],
    )?);
    raw.push_str("ORC|SC||||CM\rOBX|1|NM|SYN^^synthetic||1||||||F\r");
    let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
    let mapped = map_hl7(&message, &cfg, input())?;
    let part = first(&mapped)?;
    let path = part.source_path.as_slice();
    for (kind, status, sequence) in [
        ("MSH", Some("P"), None),
        ("SPM", None, Some("1")),
        ("OBR", Some("F"), Some("1")),
        ("ORC", Some("CM"), None),
        ("OBX", Some("F"), Some("1")),
    ] {
        let node = path
            .iter()
            .find(|n| n.kind.as_str() == kind)
            .ok_or("path node")?;
        assert_eq!(
            node.status.as_ref().map(SourceText::as_str),
            status,
            "{kind}"
        );
        assert_eq!(
            node.sequence.as_ref().map(SourceText::as_str),
            sequence,
            "{kind}"
        );
    }
    for (role, original) in [
        ("MSH.7", "202611151200"),
        ("SPM.17", "20261115110000"),
        ("SPM.18", "20261115111000"),
        ("SPM.19", "20261116110000"),
        ("OBR.7", "20261115110000"),
    ] {
        assert!(
            part.times
                .as_slice()
                .iter()
                .any(|t| t.role.as_str() == role && t.time.original.as_str() == original),
            "{role}"
        );
    }
    Ok(())
}

#[test]
fn rule3_astm_source_path_keeps_header_order_result_status_and_record_sequences() -> TestResult {
    let mut order = vec![""; 26];
    order[0] = "O";
    order[1] = "1";
    order[2] = "000AbC";
    order[25] = "F";
    let raw = format!(
        "H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\r{}\rR|1|^^^SYN|1|||||F\rL|1|N\r",
        order.join("|")
    );
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    let mapped = map_astm(
        messages.first().ok_or("message")?,
        &config(ASTM_CONFIG)?,
        input(),
    )?;
    let path = first(&mapped)?.source_path.as_slice();
    for (kind, status, sequence) in [
        ("H", Some("P"), None),
        ("P", None, Some("1")),
        ("O", Some("F"), Some("1")),
        ("R", Some("F"), Some("1")),
    ] {
        let node = path
            .iter()
            .find(|n| n.kind.as_str() == kind)
            .ok_or("path")?;
        assert_eq!(node.status.as_ref().map(SourceText::as_str), status);
        assert_eq!(node.sequence.as_ref().map(SourceText::as_str), sequence);
    }
    Ok(())
}

#[test]
fn lis02_9_9_retransmitted_and_warning_statuses_never_take_hl7_meanings() -> TestResult {
    for code in ["R", "W", "D", "U"] {
        let mapped = astm(&format!("R|1|^^^SYN|1|||||{code}\r"), "P")?;
        assert_eq!(first(&mapped)?.source_status.as_str(), code);
        assert_eq!(first(&mapped)?.status, ResultStatus::Unknown);
    }
    Ok(())
}

#[test]
fn hl7_251_2a77_declared_precision_cannot_invent_missing_digits() -> TestResult {
    for (code, expected) in [
        ("Y", TimePrecision::Year),
        ("L", TimePrecision::Month),
        ("D", TimePrecision::Day),
        ("H", TimePrecision::Hour),
        ("M", TimePrecision::Minute),
        ("S", TimePrecision::Second),
    ] {
        let mapped = hl7(
            &format!("OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000^{code}\r"),
            HL7_CONFIG,
        )?;
        let time = first(&mapped)?
            .times
            .as_slice()
            .iter()
            .find(|t| t.role.as_str() == "OBX.14")
            .ok_or("time")?;
        assert_eq!(time.time.precision, expected);
        assert_eq!(time.time.parsed.is_some(), code == "S");
        assert_eq!(
            time.time.original.as_str(),
            format!("20261115123000^{code}")
        );
    }
    Ok(())
}

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
    let definition = include_str!("../../../core/config/examples/law.toml").replace("order_namespace = {field={group=\"order\",record=\"OBR\",field=2,component=2,repetition=1}}", "order_namespace = {fixed=\"manager\"}");
    let cfg = config(&definition)?;
    let raw = b"MSH|^~\\&|A|AF|M|MF|20261115120000+0000||OUL^R22^OUL_R22|result-1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||000tube\rOBR||awos-1||SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|1|NM|SYN^Synthetic^99LAB|1^1^1|1|mg/L^mg/L^UCUM||N^Normal^HL70078|||F|||||operator||instrument^99LAB~module^99LAB|20261115120000||||||||||RSLT\r";
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
