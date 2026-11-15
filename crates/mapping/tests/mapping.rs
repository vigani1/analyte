//! Original synthetic fixtures. Sources: LIS02 sections 6,8-10; HL7 2.5.1 ch.7 and 2A.
use analyte_config::{Draft, Validated};
use analyte_mapping::{Input, Output, Problem, map_astm, map_hl7};
use analyte_model::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ASTM_CONFIG: &str = include_str!("../../config/examples/astm.toml");
const HL7_CONFIG: &str = include_str!("../../config/examples/hl7.toml");
const SITE: &str = include_str!("../../config/examples/site.toml");

fn config(definition: &str) -> Result<Validated, analyte_config::Error> {
    Draft::load(
        ("synthetic", definition),
        ("site", SITE),
        ("local", "schema_version=1"),
    )?
    .validate()
}

fn input() -> Input {
    Input {
        receipt: ReceiptId::new(),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        revision: Revision(7),
    }
}

fn hl7(body: &str, definition: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(definition)?;
    let raw = format!("MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.5.1\r{body}");
    let options = config.effective().hl7_options().ok_or("missing options")?;
    let message = analyte_hl7::Message::parse(raw.as_bytes(), options)?;
    Ok(map_hl7(&message, &config, input())?)
}

fn astm(body: &str, mode: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(ASTM_CONFIG)?;
    let raw =
        format!("H|\\^&||||||||||{mode}|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\r{body}L|1|N\r");
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    let message = messages.first().ok_or("missing parsed message")?;
    Ok(map_astm(message, &config, input())?)
}

fn first(output: &Output) -> Result<&ResultPart, Box<dyn std::error::Error>> {
    Ok(output
        .results
        .first()
        .and_then(|r| r.content.parts.as_slice().first())
        .ok_or("missing result")?)
}

#[test]
fn rule5_comparator_kept_with_original_text_lis02_9_4() -> TestResult {
    let output = astm("R|1|^^^SYN|<0.10|mg/L|0-1|H||F\r", "P")?;
    let part = first(&output)?;
    let value = part.values.as_slice().first().ok_or("missing value")?;
    assert_eq!(value.original.as_str(), "<0.10");
    assert!(
        matches!(&value.parsed, ValueInterpretation::Numeric(n) if n.number.as_str() == "0.10" && n.comparator == Some(Comparator::Less))
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

#[test]
fn rule5_locale_decimal_and_error_tokens_stay_text_for_both_protocols() -> TestResult {
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
            assert_eq!(value.parsed, ValueInterpretation::Text);
        }
    }
    Ok(())
}

#[test]
fn rule6_units_require_exact_config_match_and_no_dilution_is_guessed() -> TestResult {
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

#[test]
fn hl7_7_4_2_5_and_11_same_key_obxs_form_one_replacement_unit() -> TestResult {
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
    assert!(!output.requires_parking());
    Ok(())
}

#[test]
fn adr0003_correction_without_original_has_caller_revision() -> TestResult {
    let output = astm("R|1|^^^SYN|7|||||C\r", "P")?;
    let result = output.results.first().ok_or("missing result")?;
    assert_eq!(result.corrections.as_slice(), &[true]);
    assert!(
        matches!(&result.content.identity, IdentityStatus::Resolved(id) if id.revision == Revision(7))
    );
    Ok(())
}

#[test]
fn plan6_training_and_debug_never_deliver_for_both_protocols() -> TestResult {
    for (mode, expected) in [
        ("T", ProcessingMode::Training),
        ("D", ProcessingMode::Debug),
    ] {
        let a = astm("R|1|^^^SYN|1|||||F\r", mode)?;
        let config = config(HL7_CONFIG)?;
        let raw = format!(
            "MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|{mode}|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r"
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            config.effective().hl7_options().ok_or("options")?,
        )?;
        let b = map_hl7(&message, &config, input())?;
        for output in [a, b] {
            let result = output.results.first().ok_or("result")?;
            assert_eq!(result.content.processing, expected);
            assert!(!result.content.may_deliver_to_production());
        }
    }
    Ok(())
}

#[test]
fn rule9_missing_sample_and_unknown_code_are_positioned_findings() -> TestResult {
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

#[test]
fn hl7_ch2a_coded_structured_and_unknown_types_preserve_originals() -> TestResult {
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
            "CE" | "CWE" => assert!(matches!(value.parsed, ValueInterpretation::Coded(_))),
            "SN" => assert!(
                matches!(&value.parsed, ValueInterpretation::StructuredNumeric(n) if n.comparator == Some(Comparator::Less) && n.first.as_ref().map(Decimal::as_str) == Some("1.00"))
            ),
            _ => {
                assert!(
                    matches!(&value.parsed, ValueInterpretation::Unsupported { source_type, .. } if source_type.as_str() == "LOCAL")
                );
                assert!(output.requires_parking());
            }
        }
    }
    Ok(())
}

#[test]
fn hl7_7_4_2_11_source_statuses_are_never_replaced_by_summaries() -> TestResult {
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

#[test]
fn adr0003_run_subid_namespace_selectors_and_missing_evidence() -> TestResult {
    let definition = HL7_CONFIG.replace("sub_id = \"none\"", "sub_id = {group='observation',record='OBX',field=4,component=1,repetition=1}").replace("run = []", "run = [{group='observation',record='OBX',field=14,component=1,repetition=1}]").replace("order_namespace = {fixed=\"synthetic-orders\"}", "order_namespace = {field={group='order',record='OBR',field=3,component=2,repetition=1}}");
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

#[test]
fn hl7_7_3_7_oul_specimen_order_orc_and_result_ownership() -> TestResult {
    let config = config(HL7_CONFIG)?;
    let raw = b"MSH|^~\\&|SYN||||202611151200||OUL^R22^OUL_R22|MSG|P|2.5.1\rPID|1\rSPM|1|specimen\rSAC|||container\rOBR|1||001\rORC|RE\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message =
        analyte_hl7::Message::parse(raw, config.effective().hl7_options().ok_or("options")?)?;
    let output = map_hl7(&message, &config, input())?;
    assert_eq!(
        first(&output)?
            .source_path
            .as_slice()
            .iter()
            .map(|n| n.kind.as_str())
            .collect::<Vec<_>>(),
        ["MSH", "PID", "SPM", "OBR", "ORC", "OBX"]
    );
    assert!(!output.requires_parking());
    Ok(())
}

#[test]
fn rule8_qc_comes_only_from_config_rules() -> TestResult {
    let definition = HL7_CONFIG.replace("qc = []", "qc = [{field={group='order',record='OBR',field=3,component=1,repetition=1},value='QC001',classification='control'}]");
    let output = hl7(
        "OBR|1||QC001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
        &definition,
    )?;
    assert_eq!(first(&output)?.qc_class, QcClass::Control);
    Ok(())
}

#[test]
fn plan6_source_time_keeps_precision_zone_and_original() -> TestResult {
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
    #[test]
    fn rule5_arbitrary_parsed_messages_never_panic_or_lose_original(raw in "[a-zA-Z0-9 ,.<>=+-]{0,200}", kind in prop::sample::select(vec!["NM","ST","SN","CE","LOCAL"])) {
        let output = hl7(&format!("OBR|1||001\rOBX|1|{kind}|SYN^^synthetic||{raw}||||||F\r"), HL7_CONFIG).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let value = first(&output).map_err(|e| TestCaseError::fail(e.to_string()))?.values.as_slice().first().ok_or_else(|| TestCaseError::fail("missing original"))?;
        prop_assert_eq!(value.original.as_str(), raw.as_str());
        let output = astm(&format!("R|1|^^^SYN|{raw}|||||F\r"), "P").map_err(|e| TestCaseError::fail(e.to_string()))?;
        let value = first(&output).map_err(|e| TestCaseError::fail(e.to_string()))?.values.as_slice().first().ok_or_else(|| TestCaseError::fail("missing original"))?;
        prop_assert_eq!(value.original.as_str(), raw.as_str());
    }
}

#[test]
fn lis02_5_6_1_universal_test_id_and_repeated_values_are_retained() -> TestResult {
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

#[test]
fn lis02_9_9_all_source_statuses_are_preserved() -> TestResult {
    for code in [
        "C", "P", "F", "X", "I", "S", "M", "R", "N", "Q", "V", "W", "LOCAL",
    ] {
        let output = astm(&format!("R|1|^^^SYN|1|||||{code}\r"), "P")?;
        assert_eq!(first(&output)?.source_status.as_str(), code);
    }
    Ok(())
}

#[test]
fn hl7_ch2_escapes_and_empty_subid_declaration_keep_written_evidence() -> TestResult {
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

#[test]
fn plan6_notes_keep_parent_paths_even_without_a_result() -> TestResult {
    let output = hl7(
        "NTE|1||message note\rPID|1\rNTE|1||patient note\rOBR|1||001\rNTE|1||order note\rOBX|1|NM|SYN^^synthetic||1||||||F\rNTE|1||observation note\rOBR|2||002\rNTE|1||empty order note\r",
        HL7_CONFIG,
    )?;
    assert_eq!(output.comments.len(), 5);
    assert_eq!(
        output
            .comments
            .iter()
            .filter_map(|note| note.parent_path.as_slice().last())
            .map(|node| node.kind.as_str())
            .collect::<Vec<_>>(),
        ["MSH", "PID", "OBR", "OBX", "OBR"]
    );
    assert_eq!(
        output
            .comments
            .last()
            .and_then(|note| note.parent_path.as_slice().last())
            .map(|node| node.occurrence),
        Some(9)
    );
    Ok(())
}

#[test]
fn rule7_overlimit_repetitions_and_parts_fail_before_partial_success() -> TestResult {
    let repetitions = std::iter::repeat_n("1", 257).collect::<Vec<_>>().join("~");
    // The parser's configured limit also owns this boundary.
    let config = config(&HL7_CONFIG.replace("repetitions = 128", "repetitions = 512"))?;
    let raw = format!(
        "MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||{repetitions}||||||F\r"
    );
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        config.effective().hl7_options().ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&message, &config, input()),
        Err(analyte_mapping::Error::Limit {
            resource: "repeated values",
            ..
        })
    ));
    let mut raw = "MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.5.1\rOBR|1||001\r".to_owned();
    for ordinal in 1..=257 {
        raw.push_str(&format!("OBX|{ordinal}|NM|SYN^^synthetic||1||||||F\r"));
    }
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        config.effective().hl7_options().ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&message, &config, input()),
        Err(analyte_mapping::Error::Limit {
            resource: "replacement parts",
            ..
        })
    ));
    Ok(())
}

#[test]
fn hl7_231_oru_and_declared_ts_precision_are_kept() -> TestResult {
    let config = config(&HL7_CONFIG.replace("v251", "v231"))?;
    let raw = b"MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.3.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000^D\r";
    let message =
        analyte_hl7::Message::parse(raw, config.effective().hl7_options().ok_or("options")?)?;
    let output = map_hl7(&message, &config, input())?;
    let time = first(&output)?
        .times
        .as_slice()
        .iter()
        .find(|t| t.role.as_str() == "OBX.14")
        .ok_or("time")?;
    assert_eq!(time.time.precision, TimePrecision::Day);
    assert_eq!(time.time.original.as_str(), "20261115123000^D");
    assert!(time.time.parsed.is_none());
    Ok(())
}

proptest! {
    #[test]
    fn rule7_arbitrary_wire_inputs_that_parse_never_panic(raw in prop::collection::vec(any::<u8>(), 0..2048)) {
        let hl7_config = config(HL7_CONFIG).map_err(|e| TestCaseError::fail(e.to_string()))?;
        if let Some(options) = hl7_config.effective().hl7_options()
            && let Ok(message) = analyte_hl7::Message::parse(&raw, options)
            && let Ok(output) = map_hl7(&message, &hl7_config, input()) {
            for result in &output.results {
                for part in result.content.parts.as_slice() {
                    prop_assert!(!part.values.as_slice().is_empty());
                    for value in part.values.as_slice() {
                        if !value.original.as_str().is_empty() { prop_assert!(raw.windows(value.original.as_str().len()).any(|bytes| bytes == value.original.as_str().as_bytes())); }
                    }
                }
            }
        }
        let config = config(ASTM_CONFIG).map_err(|e| TestCaseError::fail(e.to_string()))?;
        if let Ok(messages) = analyte_astm::parse(&raw, analyte_astm::Limits::default()) {
            for message in &messages {
                if let Ok(output) = map_astm(message, &config, input()) {
                    for result in &output.results { for part in result.content.parts.as_slice() { prop_assert!(!part.values.as_slice().is_empty()); } }
                }
            }
        }
    }
}

#[test]
fn hl7_optional_orc_does_not_leak_into_the_following_order() -> TestResult {
    let definition = HL7_CONFIG.replace(
        "order_id = {group=\"order\",record=\"OBR\",field=3,component=1,repetition=1}",
        "order_id = {group='order',record='ORC',field=2,component=1,repetition=1}",
    );
    let output = hl7(
        "ORC|RE|first-order\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\rOBR|2||002\rOBX|1|NM|SYN^^synthetic||2||||||F\r",
        &definition,
    )?;
    assert!(
        matches!(&output.results.first().ok_or("first")?.content.identity, IdentityStatus::Resolved(id) if id.observation.order.value.as_str() == "first-order")
    );
    assert!(matches!(
        &output.results.get(1).ok_or("second")?.content.identity,
        IdentityStatus::Unresolved(_)
    ));
    assert!(output.findings.iter().any(|f| f.problem
        == Problem::MissingIdentity {
            setting: "order_id"
        }));
    Ok(())
}

#[test]
fn rule7_expanded_parent_context_has_a_separate_budget() -> TestResult {
    let config = config(HL7_CONFIG)?;
    let mut raw = "MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.5.1\rOBR|1||001\r".to_owned();
    raw.push_str(&format!("NTE|1||{}\r", "x".repeat(16000)));
    for ordinal in 1..=220 {
        raw.push_str(&format!(
            "OBX|{ordinal}|NM|SYN^^synthetic|{ordinal}|1||||||F\r"
        ));
    }
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        config.effective().hl7_options().ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&message, &config, input()),
        Err(analyte_mapping::Error::Limit {
            resource: "expanded evidence budget",
            ..
        })
    ));
    Ok(())
}

#[test]
fn hl7_ts_precision_indicator_cannot_hide_an_invalid_date() -> TestResult {
    let output = hl7(
        "OBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20260230123000^D\r",
        HL7_CONFIG,
    )?;
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.problem == Problem::InvalidTime)
    );
    let time = first(&output)?
        .times
        .as_slice()
        .iter()
        .find(|t| t.role.as_str() == "OBX.14")
        .ok_or("time")?;
    assert_eq!(time.time.precision, TimePrecision::Unknown);
    assert_eq!(time.time.original.as_str(), "20260230123000^D");
    Ok(())
}
