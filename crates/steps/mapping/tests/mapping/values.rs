//! Tests machine field mapping and exact value retention.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// HL7 2.9.1 §2.4.5.2
#[test]
fn truncated_input_never_reaches_mapping() -> TestResult {
    let config = config(HL7_CONFIG)?;
    let raw = b"MSH|^~\\&#|SYN||||202611161200||ORU^R01^ORU_R01|MSG|P|2.9.1\rOBR|1||001\rOBX|1|ST|SYN||partial#||||||F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    assert!(matches!(
        map_hl7(&settings::hl7_mapping(&config), &message, input()),
        Err(analyte_mapping::Error::Hl7 {
            source: analyte_hl7::Error::Truncated {
                segment: 3,
                field: 5
            },
            ..
        })
    ));
    assert_eq!(message.raw(), raw);
    Ok(())
}

// HL7 2.5.1 §2.7.4
#[test]
fn decoding_keeps_negotiated_characters() -> TestResult {
    for (charset, encoding, bytes) in [
        ("8859/1", "latin1", b"caf\xe9\\F\\end".as_slice()),
        ("UNICODE UTF-8", "utf8", "caf\u{e9}\\F\\end".as_bytes()),
    ] {
        let definition = HL7_CONFIG.rewrite(
            "encoding = \"ascii\"",
            &format!("encoding = \"{encoding}\""),
        );
        let config = config(&definition)?;
        let mut raw = format!("MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1||||||{charset}\rOBR|1||001\rOBX|1|TX|SYN^Synthetic^synthetic||").into_bytes();
        raw.extend_from_slice(bytes);
        raw.extend_from_slice(b"||||||F\r");
        let message = analyte_hl7::Message::parse(
            &raw,
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), "caf\u{e9}\\F\\end");
        assert_eq!(
            value.decoded_text().map(SourceText::as_str),
            Some("caf\u{e9}|end")
        );
        assert_eq!(message.encode(), raw);
    }
    Ok(())
}

// HL7 2.5.1 section 2.7.4 p.2-18: extract structure before decoding delimiters.
// HL7 2.5.1 §2.7.4
#[test]
fn decodes_ids_and_values_keeping_originals() -> TestResult {
    let definition = HL7_CONFIG.rewrite("machine_code=\"SYN\"", r"machine_code='A|B^C~D&E\F'");
    let config = config(&definition)?;
    let escaped = r"A\F\B\S\C\R\D\T\E\E\F";
    let raw = format!(
        "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||{escaped}\rOBX|1|TX|{escaped}^Label^synthetic||{escaped}~other||||||F\rOBX|2|CE|{escaped}^Label^synthetic||{escaped}^{escaped}^synthetic||||||F\r"
    );
    let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
    let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
    assert!(output.requires_parking());
    let result = output.results.first().ok_or("result")?;
    assert_eq!(
        result.content.sample.as_ref().ok_or("sample")?.id.as_str(),
        r"A|B^C~D&E\F"
    );
    let part = first(&output)?;
    assert_eq!(part.test.value.as_str(), "DEMO");
    assert_eq!(part.values.as_slice().len(), 2);
    let value = part.values.as_slice().first().ok_or("value")?;
    assert_eq!(value.original.as_str(), escaped);
    assert_eq!(
        value.decoded_text().map(SourceText::as_str),
        Some(r"A|B^C~D&E\F")
    );
    let coded = result
        .content
        .parts
        .as_slice()
        .get(1)
        .ok_or("coded part")?
        .values
        .as_slice()
        .first()
        .ok_or("coded value")?;
    assert_eq!(
        coded.original.as_str(),
        format!("{escaped}^{escaped}^synthetic")
    );
    assert!(
        matches!(&coded.parsed(), ValueInterpretation::Coded(code) if code.code.value.as_str() == r"A|B^C~D&E\F" && code.text.as_str() == r"A|B^C~D&E\F")
    );
    assert_eq!(message.encode(), raw.as_bytes());
    Ok(())
}

// HL7 2.5.1 sections 2.5.4, 2.7.4: use the actual escape and separator characters.
// HL7 2.5.1 §2.7.4
#[test]
fn decoding_uses_message_delimiters_once() -> TestResult {
    let config = config(HL7_CONFIG)?;
    let raw = b"MSH*$%!@*SYN****202611151200**ORU$R01$ORU_R01*MSG*P*2.5.1\rOBR*1**000!F!Ab\rOBX*1*ST*SYN$Label$synthetic**!F!!S!!R!!T!!E!%literal!E!H!E!******F\r";
    let message = analyte_hl7::Message::parse(raw, analyte_hl7::Options::default())?;
    let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
    let values = first(&output)?.values.as_slice();
    assert_eq!(values.len(), 2);
    assert_eq!(
        values
            .first()
            .and_then(|v| v.decoded_text())
            .map(SourceText::as_str),
        Some("*$%@!")
    );
    assert_eq!(
        values
            .get(1)
            .and_then(|v| v.decoded_text())
            .map(SourceText::as_str),
        Some("literal!H!")
    );
    assert_eq!(
        values.first().map(|v| v.original.as_str()),
        Some("!F!!S!!R!!T!!E!")
    );
    assert_eq!(message.encode(), raw);
    Ok(())
}

// HL7 2.5.1 sections 2.7.3, 2.7.6 pp.2-18-20.
// HL7 2.5.1 §2.7.3, §2.7.6
#[test]
fn formatting_is_unsupported_not_clean_text() -> TestResult {
    for kind in ["ST", "TX", "FT"] {
        for raw in [
            r"before\H\marked\N\after",
            r"first\.br\second",
            r"\.in+2\indented",
        ] {
            let output = hl7(
                &format!("OBR|1||001\rOBX|1|{kind}|SYN^Synthetic^synthetic||{raw}||||||F\r"),
                HL7_CONFIG,
            )?;
            let value = first(&output)?.values.as_slice().first().ok_or("value")?;
            assert_eq!(value.original.as_str(), raw);
            assert!(value.decoded_text().is_none());
            assert!(matches!(
                &value.parsed(),
                ValueInterpretation::Unsupported { .. }
            ));
            assert!(
                output
                    .findings
                    .iter()
                    .any(|finding| finding.position.record == 3
                        && finding.position.field == 5
                        && finding.problem == Problem::UnsupportedFormatting)
            );
            assert!(output.requires_parking());
        }
    }
    Ok(())
}

// Safety rule 5 (ADR 0003); CLSI LIS02-A2 §9.4
#[test]
fn comparator_kept_with_original_text() -> TestResult {
    let output = astm("R|1|^^^SYN|<0.10|mg/L|0-1|H||F\r", "P")?;
    let part = first(&output)?;
    let value = part.values.as_slice().first().ok_or("missing value")?;
    assert_eq!(value.original.as_str(), "<0.10");
    assert!(
        matches!(&value.parsed(), ValueInterpretation::Numeric(n) if n.number.as_str() == "0.10" && n.comparator == Some(Comparator::Less))
    );
    assert_eq!(part.test.value.as_str(), "DEMO");
    assert_eq!(part.qc_class, QcClass::Unknown);
    assert_eq!(
        part.reference_range.as_ref().map(SourceText::as_str),
        Some("0-1")
    );
    assert_eq!(
        part.flags.as_slice().first().map(SourceText::as_str),
        Some("H")
    );
    assert_eq!(
        output
            .results
            .first()
            .and_then(|r| r.content.sample.as_ref())
            .map(|s| s.id.as_str()),
        Some("000AbC")
    );
    assert!(!output.requires_parking());
    Ok(())
}

// Safety rule 5 (ADR 0003)
#[test]
fn locale_decimals_and_error_tokens_stay_text() -> TestResult {
    for raw in ["1,234", "positive", "ERROR", "1e3", " 12", "NaN"] {
        for output in [
            astm(&format!("R|1|^^^SYN|{raw}|||||F\r"), "P")?,
            hl7(
                &format!("OBR|1||000AbC\rOBX|1|NM|SYN^Name^synthetic||{raw}||||||F\r"),
                HL7_CONFIG,
            )?,
        ] {
            let value = first(&output)?
                .values
                .as_slice()
                .first()
                .ok_or("missing value")?;
            assert_eq!(value.original.as_str(), raw);
            assert_eq!(value.parsed(), &ValueInterpretation::Text);
        }
    }
    Ok(())
}

// Safety rule 6 (ADR 0003)
#[test]
fn units_need_exact_match_and_dilution_unguessed() -> TestResult {
    for (unit, mapped) in [("mg/L", true), ("MG/L", false), ("mg/L ", false)] {
        let output = astm(&format!("R|1|^^^SYN|1|{unit}||||F\r"), "P")?;
        let part = first(&output)?;
        let units = part.units.as_ref().ok_or("missing units")?;
        assert_eq!(units.original_text().as_str(), unit);
        assert_eq!(units.mapping().is_some(), mapped);
        assert!(part.dilution.is_none());
    }
    Ok(())
}

// HL7 §7.4.2.5, §7.4.2.11
#[test]
fn same_key_obxs_form_one_replacement() -> TestResult {
    let output = hl7(
        "PID|1\rOBR|1||001\rNTE|1||order note\rOBX|1|NM|SYN^Name^synthetic||1.20~1.20|mg/L|0-2|H||N|C\rNTE|1||result note\rOBX|2|NM|SYN^Name^synthetic||2.00||||||C\r",
        HL7_CONFIG,
    )?;
    assert_eq!(output.results.len(), 1);
    let result = output.results.first().ok_or("missing result")?;
    assert_eq!(result.content.parts.as_slice().len(), 2);
    assert_eq!(result.corrections.as_slice(), &[true, true]);
    let part = first(&output)?;
    assert_eq!(part.values.as_slice().len(), 2);
    assert_eq!(
        part.comments
            .as_slice()
            .iter()
            .map(SourceText::as_str)
            .collect::<Vec<_>>(),
        ["order note", "result note"]
    );
    assert_eq!(part.status, ResultStatus::Corrected);
    assert_eq!(part.source_status.as_str(), "C");
    assert_eq!(
        part.source_path
            .as_slice()
            .iter()
            .map(|n| n.kind.as_str())
            .collect::<Vec<_>>(),
        ["MSH", "PID", "OBR", "OBX"]
    );
    assert!(
        matches!(&result.content.identity, IdentityStatus::Resolved(id) if id.revision == Revision(7) && id.observation.run.as_str().is_empty())
    );
    assert!(output.requires_parking());
    Ok(())
}

// ADR 0003
#[test]
fn correction_without_original_keeps_caller_revision() -> TestResult {
    let output = astm("R|1|^^^SYN|7|||||C\r", "P")?;
    let result = output.results.first().ok_or("missing result")?;
    assert_eq!(result.corrections.as_slice(), &[true]);
    assert!(
        matches!(&result.content.identity, IdentityStatus::Resolved(id) if id.revision == Revision(7))
    );
    Ok(())
}

// Plan §6
#[test]
fn training_and_debug_never_deliver() -> TestResult {
    for (mode, expected) in [
        ("T", ProcessingMode::Training),
        ("D", ProcessingMode::Debug),
    ] {
        let a = astm("R|1|^^^SYN|1|||||F\r", mode)?;
        let config = config(HL7_CONFIG)?;
        let raw = format!(
            "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|{mode}|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r"
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let b = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
        for output in [a, b] {
            let result = output.results.first().ok_or("result")?;
            assert_eq!(result.content.processing, expected);
            assert!(!result.content.may_deliver_to_production());
        }
    }
    Ok(())
}

// Safety rule 9 (ADR 0003)
#[test]
fn missing_sample_and_unknown_code_are_located() -> TestResult {
    let output = hl7("OBR|1\rOBX|1|NM|SYN^^synthetic||1||||||F\r", HL7_CONFIG)?;
    assert!(output.requires_parking());
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.problem == Problem::MissingSample && f.position.field == 3)
    );
    assert!(output.findings.iter().any(|f| f.problem
        == Problem::MissingIdentity {
            setting: "order_id"
        }));
    let output = astm("R|1|^^^UNKNOWN|1|||||F\r", "P")?;
    assert!(output.results.is_empty());
    assert_eq!(output.unmapped.len(), 1);
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.problem == Problem::UnknownTest)
    );
    assert_eq!(
        output
            .unmapped
            .first()
            .and_then(|r| r.values.as_slice().first())
            .map(|v| v.original.as_str()),
        Some("1")
    );
    Ok(())
}

// HL7 chapter 2A
#[test]
fn coded_structured_and_unknown_types_keep_originals() -> TestResult {
    for (kind, raw) in [
        ("CE", "POS^positive^synthetic"),
        ("CWE", "NEG^negative^synthetic"),
        ("SN", "<^1.00^:^2.00"),
        ("LOCAL", "unchanged"),
    ] {
        let output = hl7(
            &format!("OBR|1||001\rOBX|1|{kind}|SYN^^synthetic||{raw}||||||F\r"),
            HL7_CONFIG,
        )?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), raw);
        match kind {
            "CE" | "CWE" => assert!(matches!(value.parsed(), ValueInterpretation::Coded(_))),
            "SN" => assert!(
                matches!(&value.parsed(), ValueInterpretation::StructuredNumeric(n) if n.comparator == Some(Comparator::Less) && n.first.as_ref().map(Decimal::as_str) == Some("1.00"))
            ),
            _ => {
                assert!(
                    matches!(&value.parsed(), ValueInterpretation::Unsupported { source_type, .. } if source_type.as_str() == "LOCAL")
                );
                assert!(output.requires_parking());
            }
        }
    }
    Ok(())
}

// HL7 §7.4.2.11
#[test]
fn obx11_statuses_are_never_replaced() -> TestResult {
    for (code, status) in [
        ("C", ResultStatus::Corrected),
        ("D", ResultStatus::Withdrawn),
        ("F", ResultStatus::Final),
        ("I", ResultStatus::Pending),
        ("N", ResultStatus::Unknown),
        ("O", ResultStatus::Unknown),
        ("P", ResultStatus::Preliminary),
        ("R", ResultStatus::Preliminary),
        ("S", ResultStatus::Preliminary),
        ("X", ResultStatus::NotPerformed),
        ("U", ResultStatus::Final),
        ("W", ResultStatus::Withdrawn),
        ("LOCAL", ResultStatus::Unknown),
    ] {
        let output = hl7(
            &format!("OBR|1||001\rOBX|1|NM|SYN^^synthetic||||||||{code}\r"),
            HL7_CONFIG,
        )?;
        assert_eq!(first(&output)?.source_status.as_str(), code);
        assert_eq!(first(&output)?.status, status);
        assert_eq!(
            first(&output)?
                .values
                .as_slice()
                .first()
                .map(|v| v.original.as_str()),
            Some("")
        );
    }
    Ok(())
}

// ADR 0003
#[test]
fn identity_selectors_resolve_or_stay_unresolved() -> TestResult {
    let definition = HL7_CONFIG.rewrite("sub_id = \"none\"", "sub_id = {group='observation',record='OBX',field=4,component=1,repetition=1}").rewrite("run = []", "run = [{group='observation',record='OBX',field=14,component=1,repetition=1}]").rewrite("order_namespace = {fixed=\"synthetic-orders\"}", "order_namespace = {field={group='order',record='OBR',field=3,component=2,repetition=1}}");
    let output = hl7(
        "OBR|1||001^orders\rOBX|1|NM|SYN^^synthetic|a|1||||||F|||20261115123000\rOBX|2|NM|SYN^^synthetic|a|1||||||F|||20261115123100\r",
        &definition,
    )?;
    assert_eq!(output.results.len(), 2);
    assert!(
        matches!(&output.results.first().ok_or("result")?.content.identity, IdentityStatus::Resolved(id) if id.observation.order.namespace.as_str() == "orders" && id.observation.sub_id.as_str() == "a" && id.observation.run.as_str() == "14:20261115123000")
    );
    let missing = hl7(
        "OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
        &definition,
    )?;
    for setting in ["sub_id", "run", "order_namespace"] {
        assert!(
            missing
                .findings
                .iter()
                .any(|f| f.problem == Problem::MissingIdentity { setting })
        );
    }
    assert!(
        matches!(&missing.results.first().ok_or("result")?.content.identity, IdentityStatus::Unresolved(reason) if reason.as_str().contains("run"))
    );
    Ok(())
}

#[test]
fn hl7_group_selection_never_crosses_orders() -> TestResult {
    let output = hl7(
        "PID|1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\rOBR|2||002\rOBX|1|NM|SYN^^synthetic||2||||||F\r",
        HL7_CONFIG,
    )?;
    assert_eq!(output.results.len(), 2);
    let samples = output
        .results
        .iter()
        .filter_map(|r| r.content.sample.as_ref())
        .map(|s| s.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(samples, ["001", "002"]);
    Ok(())
}

// HL7 §7.3.7
#[test]
fn oul_result_path_keeps_specimen_and_order() -> TestResult {
    let config = config(HL7_CONFIG)?;
    let raw = b"MSH|^~\\&|SYN||||202611151200||OUL^R22^OUL_R22|MSG|P|2.5.1\rPID|1\rSPM|1|specimen\rSAC|||container\rOBR|1||001\rORC|RE\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message =
        analyte_hl7::Message::parse(raw, effective_options(config.effective()).ok_or("options")?)?;
    let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
    assert_eq!(
        first(&output)?
            .source_path
            .as_slice()
            .iter()
            .map(|n| n.kind.as_str())
            .collect::<Vec<_>>(),
        ["MSH", "PID", "SPM", "OBR", "ORC", "OBX"]
    );
    assert!(output.requires_parking());
    Ok(())
}

// Safety rule 8 (ADR 0003)
#[test]
fn qc_comes_only_from_config() -> TestResult {
    let definition = HL7_CONFIG.rewrite("qc = []", "qc = [{field={group='order',record='OBR',field=3,component=1,repetition=1},value='QC001',classification='control'}]");
    let output = hl7(
        "OBR|1||QC001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
        &definition,
    )?;
    assert_eq!(first(&output)?.qc_class, QcClass::Control);
    Ok(())
}

// Plan §6
#[test]
fn source_time_keeps_precision_zone_and_original() -> TestResult {
    let output = hl7(
        "OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000.1200+0130\r",
        HL7_CONFIG,
    )?;
    let time = first(&output)?
        .times
        .as_slice()
        .iter()
        .find(|t| t.role.as_str() == "OBX.14")
        .ok_or("time")?;
    assert_eq!(time.time.original.as_str(), "20261115123000.1200+0130");
    assert_eq!(time.time.offset.map(i32::from), Some(5400));
    assert_eq!(time.time.precision, TimePrecision::FractionalSecond);
    assert!(time.time.parsed.is_some());
    Ok(())
}

proptest! {
    // Safety rule 5 (ADR 0003)
    #[test]
    fn arbitrary_values_never_panic_or_lose_original(raw in "[a-zA-Z0-9 ,.<>=+-]{0,200}", kind in prop::sample::select(vec!["NM","ST","SN","CE","LOCAL"])) {
        let output = hl7(&format!("OBR|1||001\rOBX|1|{kind}|SYN^^synthetic||{raw}||||||F\r"), HL7_CONFIG).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let value = first(&output).map_err(|e| TestCaseError::fail(e.to_string()))?.values.as_slice().first().ok_or_else(|| TestCaseError::fail("missing original"))?;
        prop_assert_eq!(value.original.as_str(), raw.as_str());
        let output = astm(&format!("R|1|^^^SYN|{raw}|||||F\r"), "P").map_err(|e| TestCaseError::fail(e.to_string()))?;
        let value = first(&output).map_err(|e| TestCaseError::fail(e.to_string()))?.values.as_slice().first().ok_or_else(|| TestCaseError::fail("missing original"))?;
        prop_assert_eq!(value.original.as_str(), raw.as_str());
    }
}

// CLSI LIS02-A2 §5.6.1
#[test]
fn universal_test_id_and_repeats_are_kept() -> TestResult {
    let output = astm("R|1|SYN|1.0\\1.0|mg/L|low\\high|H\\L||F\r", "P")?;
    let part = first(&output)?;
    assert_eq!(
        part.values
            .as_slice()
            .iter()
            .map(|v| v.original.as_str())
            .collect::<Vec<_>>(),
        ["1.0", "1.0"]
    );
    assert_eq!(
        part.flags
            .as_slice()
            .iter()
            .map(SourceText::as_str)
            .collect::<Vec<_>>(),
        ["H", "L"]
    );
    assert_eq!(
        part.reference_range.as_ref().map(SourceText::as_str),
        Some("low\\high")
    );
    Ok(())
}

// CLSI LIS02-A2 §9.9
#[test]
fn astm_source_statuses_are_all_kept() -> TestResult {
    for code in [
        "C", "P", "F", "X", "I", "S", "M", "R", "N", "Q", "V", "W", "LOCAL",
    ] {
        let output = astm(&format!("R|1|^^^SYN|1|||||{code}\r"), "P")?;
        assert_eq!(first(&output)?.source_status.as_str(), code);
    }
    Ok(())
}

// HL7 chapter 2
#[test]
fn escapes_and_empty_subid_keep_written_values() -> TestResult {
    let output = hl7(
        "OBR|1||001\rOBX|1|ST|SYN^^synthetic||a\\F\\b~a\\S\\b||||||F\r",
        HL7_CONFIG,
    )?;
    let values = first(&output)?.values.as_slice();
    assert_eq!(
        values
            .iter()
            .map(|v| v.original.as_str())
            .collect::<Vec<_>>(),
        ["a\\F\\b", "a\\S\\b"]
    );
    Ok(())
}

// Plan §6
#[test]
fn notes_keep_parent_paths_without_a_result() -> TestResult {
    // HL7 2.5.1 section 7.3.1 does not admit an ORU header-level NTE.
    assert!(hl7("NTE|1||message note\rOBR|1||001\r", HL7_CONFIG).is_err());
    let output = hl7(
        "PID|1\rNTE|1||patient note\rOBR|1||001\rNTE|1||order note\rOBX|1|NM|SYN^^synthetic||1||||||F\rNTE|1||observation note\rOBR|2||002\rNTE|1||empty order note\r",
        HL7_CONFIG,
    )?;
    assert_eq!(output.comments.len(), 4);
    assert_eq!(
        output
            .comments
            .iter()
            .filter_map(|note| note.parent_path.as_slice().last())
            .map(|node| node.kind.as_str())
            .collect::<Vec<_>>(),
        ["PID", "OBR", "OBX", "OBR"]
    );
    assert_eq!(
        output
            .comments
            .last()
            .and_then(|note| note.parent_path.as_slice().last())
            .map(|node| node.occurrence),
        Some(8)
    );
    Ok(())
}
