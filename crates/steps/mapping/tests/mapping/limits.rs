//! Tests machine field mapping and exact value retention.
use super::*;
use analyte_fixtures::ConfigEdit as _;

#[test]
fn rule7_overlimit_repetitions_and_parts_fail_before_partial_success() -> TestResult {
    let repetitions = std::iter::repeat_n("1", 257).collect::<Vec<_>>().join("~");
    // The parser's configured limit also owns this boundary.
    let config = config(&HL7_CONFIG.rewrite("repetitions = 128", "repetitions = 512"))?;
    let raw = format!(
        "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||{repetitions}||||||F\r"
    );
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        config.effective().hl7_options().ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(
            &message,
            settings::hl7(&config).0,
            settings::hl7(&config).1,
            &config.effective().machine().deviations,
            input()
        ),
        Err(analyte_mapping::Error::Limit {
            resource: "repeated values",
            ..
        })
    ));
    let mut raw =
        "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\r".to_owned();
    for ordinal in 1..=257 {
        raw.push_str(&format!("OBX|{ordinal}|NM|SYN^^synthetic||1||||||F\r"));
    }
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        config.effective().hl7_options().ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(
            &message,
            settings::hl7(&config).0,
            settings::hl7(&config).1,
            &config.effective().machine().deviations,
            input()
        ),
        Err(analyte_mapping::Error::Limit {
            resource: "replacement parts",
            ..
        })
    ));
    Ok(())
}

#[test]
fn hl7_231_oru_and_declared_ts_precision_are_kept() -> TestResult {
    let config = config(&HL7_CONFIG.rewrite("v251", "v231"))?;
    let raw = b"MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.3.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000^D\r";
    let message =
        analyte_hl7::Message::parse(raw, config.effective().hl7_options().ok_or("options")?)?;
    let output = map_hl7(
        &message,
        settings::hl7(&config).0,
        settings::hl7(&config).1,
        &config.effective().machine().deviations,
        input(),
    )?;
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
            && let Ok(output) = map_hl7(&message, settings::hl7(&hl7_config).0, settings::hl7(&hl7_config).1, &hl7_config.effective().machine().deviations, input()) {
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
                if let Ok(output) = map_astm(message, settings::astm(&config).0, settings::astm(&config).1, input()) {
                    for result in &output.results { for part in result.content.parts.as_slice() { prop_assert!(!part.values.as_slice().is_empty()); } }
                }
            }
        }
    }
}

#[test]
fn hl7_optional_orc_does_not_leak_into_the_following_order() -> TestResult {
    let definition = HL7_CONFIG.rewrite(
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
    let mut raw =
        "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\r".to_owned();
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
        map_hl7(
            &message,
            settings::hl7(&config).0,
            settings::hl7(&config).1,
            &config.effective().machine().deviations,
            input()
        ),
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

#[test]
fn lis02_6_12_hl7_251_msh11_unknown_processing_still_refused() -> TestResult {
    for mode in ["", "U"] {
        let definition = config(ASTM_CONFIG)?;
        let raw = format!(
            "H|\\^&||||||||||{mode}|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^SYN|1|mg/L||||F\rL|1|N\r"
        );
        let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
        let message = messages.first().ok_or("missing message")?;
        assert!(matches!(
            map_astm(
                message,
                settings::astm(&definition).0,
                settings::astm(&definition).1,
                input()
            ),
            Err(analyte_mapping::Error::Structure {
                reason: "unknown processing mode",
                ..
            })
        ));
        let definition = config(HL7_CONFIG)?;
        let raw = format!(
            "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|{mode}|2.5.1\rOBR|1||sample\rOBX|1|NM|SYN^Synthetic^synthetic||1|mg/L|||||F\r"
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            definition.effective().hl7_options().ok_or("options")?,
        )?;
        assert!(matches!(
            map_hl7(
                &message,
                settings::hl7(&definition).0,
                settings::hl7(&definition).1,
                &definition.effective().machine().deviations,
                input()
            ),
            Err(analyte_mapping::Error::Hl7 {
                source: analyte_hl7::Error::Control {
                    segment: "MSH",
                    field: 11
                },
                ..
            })
        ));
    }
    Ok(())
}

#[test]
fn lis02_5_6_1_component_parts_and_rule6_exact_dilution() -> TestResult {
    for original in ["02.00", "1:10", "", " 2", "not"] {
        let output = with_parts(&format!("SYN/{original}"), &parts_config())?;
        assert!(!output.requires_parking());
        let dilution = first(&output)?
            .dilution
            .as_ref()
            .ok_or("missing dilution")?;
        assert_eq!(
            dilution.original.as_ref().map(SourceText::as_str),
            Some(original)
        );
        assert_eq!(
            dilution.factor.as_ref().map(Decimal::as_str),
            if original == "02.00" {
                Some(original)
            } else {
                None
            }
        );
        assert_eq!(dilution.applied, DilutionApplied::Unknown);
    }
    for original in ["SYN", "SYN/2/extra"] {
        let output = with_parts(original, &parts_config())?;
        assert!(output.requires_parking());
        assert!(
            output
                .findings
                .iter()
                .any(|f| f.position.field == 3 && f.problem == Problem::ComponentParts)
        );
        assert_eq!(output.unmapped.len(), 1);
    }
    Ok(())
}

#[test]
fn lis02_9_4_and_hl7_7_4_2_value_expectations_preserve_originals() -> TestResult {
    for (kind, raw, mismatch) in [
        ("numeric", "positive", true),
        ("numeric", "<0.10", false),
        ("coded", "positive", false),
        ("coded", "12", true),
        ("either", "12", false),
    ] {
        for (definition, astm_source) in [(ASTM_CONFIG, true), (HL7_CONFIG, false)] {
            let definition = definition.rewrite(
                "machine_code=",
                &format!("value_kind='{kind}', machine_code="),
            );
            let output = if astm_source {
                let config = config(&definition)?;
                let message = format!(
                    "H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^SYN|{raw}|||||F\rL|1|N\r"
                );
                let messages =
                    analyte_astm::parse(message.as_bytes(), analyte_astm::Limits::default())?;
                map_astm(
                    messages.first().ok_or("missing message")?,
                    settings::astm(&config).0,
                    settings::astm(&config).1,
                    input(),
                )?
            } else {
                qualified_hl7(
                    &format!("OBR|1||000AbC\rOBX|1|NM|SYN^Name^synthetic||{raw}||||||F\r"),
                    &definition,
                )?
            };
            // HL7 NM does not accept comparator prefixes as plain decimals.
            let expected = mismatch || (!astm_source && kind == "numeric" && raw == "<0.10");
            assert_eq!(output.requires_parking(), expected);
            assert_eq!(
                first(&output)?
                    .values
                    .as_slice()
                    .first()
                    .ok_or("missing value")?
                    .original
                    .as_str(),
                raw
            );
            assert_eq!(
                output
                    .findings
                    .iter()
                    .any(|f| matches!(f.problem, Problem::ValueKindMismatch { .. })),
                expected
            );
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn lis02_5_6_1_arbitrary_component_preserves_dilution(original in "[a-zA-Z0-9/: .+-]{0,100}") {
        let output = with_parts(&format!("SYN/{original}"), &parts_config()).expect("bounded synthetic input");
        if original.contains('/') {
            prop_assert!(output.requires_parking());
        } else {
            let part = first(&output).expect("mapped test");
            prop_assert_eq!(part.dilution.as_ref().and_then(|d| d.original.as_ref()).map(SourceText::as_str), Some(original.as_str()));
        }
    }
}

#[test]
fn rule6_application_comes_only_from_configured_source_evidence() -> TestResult {
    for (yes, no, expected) in [
        ("F", "N", DilutionApplied::Yes),
        ("Y", "F", DilutionApplied::No),
        ("Y", "N", DilutionApplied::Unknown),
    ] {
        let definition = format!(
            "{}applied = {{field={{group='observation', record='R', field=9, component=1, repetition=1}}, yes='{yes}', no='{no}'}}\n",
            parts_config()
        );
        let output = with_parts("SYN/1:10", &definition)?;
        assert_eq!(
            first(&output)?
                .dilution
                .as_ref()
                .ok_or("missing dilution")?
                .applied,
            expected
        );
    }
    Ok(())
}

// HL7 2.5.1 sections 2.7.2, 2.7.5, 2.7.7: no repertoire/local/hex agreement is inferred.
#[test]
fn hl7_251_2_7_unsupported_escapes_never_become_clean_text() -> TestResult {
    for raw in [r"A\X41\B", r"A\Zlocal\B", r"A\C2842\B", r"A\M2442\B"] {
        let output = hl7(
            &format!("OBR|1||001\rOBX|1|TX|SYN^Synthetic^synthetic||{raw}||||||F\r"),
            HL7_CONFIG,
        )?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), raw);
        assert!(value.decoded_text().is_none());
        assert!(
            output
                .findings
                .iter()
                .any(|f| f.problem == Problem::UnsupportedEscape)
        );
        assert!(output.requires_parking());
    }
    Ok(())
}

// LIS02-A2 sections 10 and 4.1: consecutive comments share the noncomment owner.
#[test]
fn lis02_10_consecutive_comments_share_noncomment_owner() -> TestResult {
    let body = "R|1|^^^SYN|1|||||F\rC|1|I|first|G\rC|2|I|second|G\r";
    let wire = format!("H|\\^&|\rP|1\rO|1\r{body}L|1\r");
    let messages = analyte_astm::parse(wire.as_bytes(), analyte_astm::Limits::default())?;
    let message = messages.first().ok_or("message")?;
    assert_eq!(
        message
            .records()
            .get(4)
            .and_then(analyte_astm::Record::parent),
        Some(3)
    );
    assert_eq!(
        message
            .records()
            .get(5)
            .and_then(analyte_astm::Record::parent),
        Some(3)
    );
    assert_eq!(message.encode(), wire.as_bytes());
    let output = astm(body, "P")?;
    assert_eq!(
        first(&output)?
            .comments
            .as_slice()
            .iter()
            .map(SourceText::as_str)
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    Ok(())
}

#[test]
fn lis02_6_12_composite_processing_id_never_authorizes_production() -> TestResult {
    for mode in ["P^T", "P\\T", "P&F&", "UNKNOWN"] {
        assert!(astm("R|1|^^^SYN|1|mg/L||||F\r", mode).is_err());
    }
    Ok(())
}

#[test]
fn lis02_5_4_5_8_4_3_escaped_specimen_matches_decoded_identity() -> TestResult {
    let definition = config(ASTM_CONFIG)?;
    for (raw_id, decoded) in [
        ("A&F&B", "A|B"),
        ("A&S&B", "A^B"),
        ("A&R&B", "A\\B"),
        ("A&E&B", "A&B"),
    ] {
        let raw = format!(
            "H|\\^&||||||||||P|LIS2-A2\rP|1\rO|1|{raw_id}\rR|1|^^^SYN|1|mg/L||||F\rL|1|N\r"
        );
        let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
        let output = map_astm(
            &messages[0],
            settings::astm(&definition).0,
            settings::astm(&definition).1,
            input(),
        )?;
        let sample = output.results[0].content.sample.as_ref().ok_or("sample")?;
        assert_eq!(sample.id.as_str(), decoded);
        assert_eq!(messages[0].encode(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn lis02_5_4_5_unagreed_identity_escape_is_refused() -> TestResult {
    let definition = config(ASTM_CONFIG)?;
    let raw = b"H|\\^&||||||||||P|LIS2-A2\rP|1\rO|1|A&Zlocal&B\rR|1|^^^SYN|1|mg/L||||F\rL|1|N\r";
    let messages = analyte_astm::parse(raw, analyte_astm::Limits::default())?;
    assert!(
        map_astm(
            &messages[0],
            settings::astm(&definition).0,
            settings::astm(&definition).1,
            input()
        )
        .is_err()
    );
    assert_eq!(messages[0].encode(), raw);
    Ok(())
}
