//! Regression scenarios for mutation.
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
