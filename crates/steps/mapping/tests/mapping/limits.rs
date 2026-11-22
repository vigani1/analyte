//! Tests machine field mapping and exact value retention.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// Safety rule 7 (ADR 0003)
#[test]
fn overlimit_repetitions_and_parts_fail_whole() -> TestResult {
    let repetitions = std::iter::repeat_n("1", 257).collect::<Vec<_>>().join("~");
    // The parser's configured limit also owns this boundary.
    let config = config(&HL7_CONFIG.rewrite("repetitions = 128", "repetitions = 512"))?;
    let raw = format!(
        "MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||{repetitions}||||||F\r"
    );
    let message = analyte_hl7::Message::parse(
        raw.as_bytes(),
        effective_options(config.effective()).ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&settings::hl7_mapping(&config), &message, input()),
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
        effective_options(config.effective()).ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&settings::hl7_mapping(&config), &message, input()),
        Err(analyte_mapping::Error::Limit {
            resource: "replacement parts",
            ..
        })
    ));
    Ok(())
}

// HL7 2.3.1
#[test]
fn legacy_oru_keeps_declared_ts_precision() -> TestResult {
    let config = config(&HL7_CONFIG.rewrite("v251", "v231"))?;
    let raw = b"MSH|^~\\&|SYN||||202611151200||ORU^R01|MSG|P|2.3.1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F|||20261115123000^D\r";
    let message =
        analyte_hl7::Message::parse(raw, effective_options(config.effective()).ok_or("options")?)?;
    let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
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
    // Safety rule 7 (ADR 0003)
    #[test]
    fn parsed_wire_inputs_never_panic(raw in prop::collection::vec(any::<u8>(), 0..2048)) {
        let hl7_config = config(HL7_CONFIG).map_err(|e| TestCaseError::fail(e.to_string()))?;
        if let Some(options) = effective_options(hl7_config.effective())
            && let Ok(message) = analyte_hl7::Message::parse(&raw, options)
            && let Ok(output) = map_hl7(&analyte_mapping::hl7::Settings { syntax: settings::hl7(&hl7_config).0, mappings: settings::hl7(&hl7_config).1, declarations: &hl7_config.effective().machine().deviations }, &message, input()) {
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
                if let Ok(output) = map_astm(&analyte_mapping::astm::Settings { syntax: settings::astm(&config).0, mappings: settings::astm(&config).1 }, message, input()) {
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
            setting: analyte_mapping::IdentitySetting::OrderId
        }));
    Ok(())
}

// Safety rule 7 (ADR 0003)
#[test]
fn parent_context_has_a_separate_budget() -> TestResult {
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
        effective_options(config.effective()).ok_or("options")?,
    )?;
    assert!(matches!(
        map_hl7(&settings::hl7_mapping(&config), &message, input()),
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

// CLSI LIS02-A2 §6.12; HL7 2.5.1 MSH-11
#[test]
fn unknown_processing_mode_is_refused() -> TestResult {
    for mode in ["", "U"] {
        let definition = config(ASTM_CONFIG)?;
        let raw = format!(
            "H|\\^&||||||||||{mode}|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^SYN|1|mg/L||||F\rL|1|N\r"
        );
        let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
        let message = messages.first().ok_or("missing message")?;
        assert!(matches!(
            map_astm(
                &analyte_mapping::astm::Settings {
                    syntax: settings::astm(&definition).0,
                    mappings: settings::astm(&definition).1
                },
                message,
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
            effective_options(definition.effective()).ok_or("options")?,
        )?;
        assert!(matches!(
            map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&definition).0,
                    mappings: settings::hl7(&definition).1,
                    declarations: &definition.effective().machine().deviations
                },
                &message,
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

// CLSI LIS02-A2 §5.6.1; Safety rule 6 (ADR 0003)
#[test]
fn component_parts_keep_exact_dilution() -> TestResult {
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
        assert_eq!(output.uncoded.len(), 1);
    }
    Ok(())
}

// CLSI LIS02-A2 §9.4; HL7 §7.4.2
#[test]
fn value_kind_mismatch_keeps_originals() -> TestResult {
    for (kind, raw, mismatch) in [
        ("numeric", "positive", true),
        ("numeric", "<0.10", false),
        ("coded", "positive", false),
        ("coded", "12", true),
        ("either", "12", false),
    ] {
        for (definition, astm_source) in [(ASTM_CONFIG, true), (HL7_CONFIG, false)] {
            // "either" is the absence of an expectation (ADR 0029).
            let definition = if kind == "either" {
                definition.to_owned()
            } else {
                definition.rewrite(
                    "test_system = \"synthetic\"",
                    &format!(
                        "test_system = \"synthetic\"\nvalue_kinds = [{{code=\"SYN\", kind=\"{kind}\"}}]"
                    ),
                )
            };
            let output = if astm_source {
                let config = config(&definition)?;
                let message = format!(
                    "H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^SYN|{raw}|||||F\rL|1|N\r"
                );
                let messages =
                    analyte_astm::parse(message.as_bytes(), analyte_astm::Limits::default())?;
                map_astm(
                    &settings::astm_mapping(&config),
                    messages.first().ok_or("missing message")?,
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
    // CLSI LIS02-A2 §5.6.1
    #[test]
    fn arbitrary_component_keeps_dilution(original in "[a-zA-Z0-9/: .+-]{0,100}") {
        let output = with_parts(&format!("SYN/{original}"), &parts_config()).expect("bounded synthetic input");
        if original.contains('/') {
            prop_assert!(output.requires_parking());
        } else {
            let part = first(&output).expect("mapped test");
            prop_assert_eq!(part.dilution.as_ref().and_then(|d| d.original.as_ref()).map(SourceText::as_str), Some(original.as_str()));
        }
    }
}

// Safety rule 6 (ADR 0003)
#[test]
fn dilution_applied_only_from_configured_evidence() -> TestResult {
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
// HL7 2.5.1 §2.7
#[test]
fn unsupported_escapes_never_become_clean_text() -> TestResult {
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
// CLSI LIS02-A2 §10
#[test]
fn consecutive_comments_share_their_owner() -> TestResult {
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

// CLSI LIS02-A2 §6.12
#[test]
fn composite_processing_id_is_refused() -> TestResult {
    for mode in ["P^T", "P\\T", "P&F&", "UNKNOWN"] {
        assert!(astm("R|1|^^^SYN|1|mg/L||||F\r", mode).is_err());
    }
    Ok(())
}

// CLSI LIS02-A2 §5.4.5, §8.4.3
#[test]
fn escaped_specimen_id_is_decoded() -> TestResult {
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
        let output = map_astm(&settings::astm_mapping(&definition), &messages[0], input())?;
        let sample = output.results[0].content.sample.as_ref().ok_or("sample")?;
        assert_eq!(sample.id.as_str(), decoded);
        assert_eq!(messages[0].encode(), raw.as_bytes());
    }
    Ok(())
}

// CLSI LIS02-A2 §5.4.5
#[test]
fn unagreed_identity_escape_is_refused() -> TestResult {
    let definition = config(ASTM_CONFIG)?;
    let raw = b"H|\\^&||||||||||P|LIS2-A2\rP|1\rO|1|A&Zlocal&B\rR|1|^^^SYN|1|mg/L||||F\rL|1|N\r";
    let messages = analyte_astm::parse(raw, analyte_astm::Limits::default())?;
    assert!(
        map_astm(
            &analyte_mapping::astm::Settings {
                syntax: settings::astm(&definition).0,
                mappings: settings::astm(&definition).1
            },
            &messages[0],
            input()
        )
        .is_err()
    );
    assert_eq!(messages[0].encode(), raw);
    Ok(())
}
