//! Applies documented upload layouts to synthetic ASTM results.
use super::*;

fn upload(
    layout: &str,
    order: &str,
    body: &str,
    completion: &str,
    kind: Option<&str>,
) -> Result<Output, Box<dyn std::error::Error>> {
    let mut definition = ASTM_CONFIG.replace(
        "correction_status_codes = [\"C\"]",
        "correction_status_codes = []",
    );
    if layout == "measurement_marker" {
        definition = definition.replace(
            "group = \"order\"\nrecord = \"O\"\nfield = 3",
            "group = \"patient\"\nrecord = \"P\"\nfield = 4",
        );
        definition = definition.replace(
            "order_id = {group=\"order\",record=\"O\",field=3",
            "order_id = {group=\"patient\",record=\"P\",field=4",
        );
    }
    if let Some(kind) = kind {
        definition.push_str(&format!(
            "\n[[mappings.value_kinds]]\ncode='SYN'\nkind='{kind}'\n"
        ));
    }
    definition.push_str(&format!("\n[mappings.astm_results]\nlayout='{layout}'\nevidence={{manual='Synthetic regression citation',section='Result upload'}}\n"));
    let config = config(&definition)?;
    let raw = format!(
        "H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1||000PaT\r{order}\r{body}L|1|{completion}\r"
    );
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    Ok(map_astm(
        &settings::astm_mapping(&config),
        messages.first().ok_or("message")?,
        input(),
    )?)
}

const CARET: &str = "caret_dilution_and_alarm";
const SLASH: &str = "slash_dilution_and_alarm_code";
const SLASH_LOT: &str = "slash_dilution_and_alarm_code_with_qc_lot";
const MARKER: &str = "measurement_marker";
const CARET_PATIENT: &str = "O|1|000aBc|7^A1^1^^SAMPLE^NORMAL||||||||X";
const SLASH_PATIENT: &str = "O|1|000aBc|7^A1^1^^S1^SC||||||||N";

// Roche e 411 HIM v2.3 section 6 Table 18.
#[test]
fn qualitative_code_and_cutoff_keep_distinct_meanings() -> TestResult {
    for (layout, order, code) in [
        (CARET, CARET_PATIENT, "SYN^1^0"),
        (SLASH, SLASH_PATIENT, "SYN/2/not"),
    ] {
        for (token, reaction) in [
            ("-1", QualitativeReaction::Negative),
            ("0", QualitativeReaction::Borderline),
            ("1", QualitativeReaction::Positive),
        ] {
            let original = format!("{token}^0.900");
            let output = upload(
                layout,
                order,
                &format!("R|1|^^^{code}|{original}\r"),
                "N",
                Some("coded"),
            )?;
            let part = first(&output)?;
            let value = part.values.as_slice().first().ok_or("value")?;
            assert_eq!(value.original.as_str(), original);
            assert_eq!(value.parsed(), &ValueInterpretation::Qualitative(reaction));
            let cutoff = part
                .details
                .as_ref()
                .and_then(|details| details.cutoff_index.as_ref())
                .ok_or("cutoff")?;
            assert!(
                matches!(cutoff.parsed(), ValueInterpretation::Numeric(n) if n.number.as_str() == "0.900")
            );
        }
    }
    Ok(())
}

// Roche e 411 HIM v2.3 section 6 Table 18; AGENTS.md clinical rules 2 and 5.
#[test]
fn undeclared_qualitative_tokens_are_retained_as_unsupported() -> TestResult {
    for token in ["2", "+1"] {
        let original = format!("{token}^0.900");
        let output = upload(
            SLASH,
            SLASH_PATIENT,
            &format!("R|1|^^^SYN/2/not|{original}\r"),
            "N",
            Some("coded"),
        )?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), original);
        assert!(matches!(
            value.parsed(),
            ValueInterpretation::Unsupported { source_type, .. }
                if source_type.as_str() == "ASTM R.4"
        ));
        assert!(
            output
                .findings
                .iter()
                .any(|finding| finding.problem == Problem::UnsupportedValue)
        );
        assert!(output.requires_parking());
    }
    Ok(())
}

// Roche e 411 HIM v2.3 section 6 Table 18.
#[test]
fn quantitative_comparator_and_message_code_stay_separate() -> TestResult {
    let output = upload(
        SLASH,
        SLASH_PATIENT,
        "R|1|^^^SYN/2/not|<0.12^07|mg/L||||C\r",
        "N",
        Some("numeric"),
    )?;
    let part = first(&output)?;
    let value = part.values.as_slice().first().ok_or("value")?;
    assert_eq!(value.original.as_str(), "<0.12^07");
    assert!(
        matches!(value.parsed(), ValueInterpretation::Numeric(n) if n.comparator == Some(Comparator::Less))
    );
    assert_eq!(
        part.details
            .as_ref()
            .and_then(|details| details.message_code.as_ref())
            .map(SourceText::as_str),
        Some("07")
    );
    assert_eq!(part.status, ResultStatus::Unknown);
    assert_eq!(part.source_status.as_str(), "C");
    assert_eq!(
        output
            .results
            .first()
            .ok_or("result")?
            .corrections
            .as_slice(),
        &[false]
    );
    let held = upload(SLASH, SLASH_PATIENT, "R|1|^^^SYN/2/not|0^07\r", "N", None)?;
    assert!(held.requires_parking());
    Ok(())
}

// Roche e 411 HIM v2.3 section 6 Table 18.
#[test]
fn seven_spaces_are_absent_instead_of_a_numeric_value() -> TestResult {
    for (layout, order, code) in [
        (CARET, CARET_PATIENT, "SYN^1^0"),
        (SLASH, SLASH_PATIENT, "SYN/2/not"),
    ] {
        let output = upload(
            layout,
            order,
            &format!("R|1|^^^{code}|       \r"),
            "N",
            Some("numeric"),
        )?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), "       ");
        assert_eq!(value.parsed(), &ValueInterpretation::Absent);
        let output = upload(
            layout,
            order,
            &format!("R|1|^^^{code}|1^       \r"),
            "N",
            Some("coded"),
        )?;
        let cutoff = first(&output)?
            .details
            .as_ref()
            .and_then(|details| details.cutoff_index.as_ref())
            .ok_or("cutoff")?;
        assert_eq!(cutoff.original.as_str(), "       ");
        assert_eq!(cutoff.parsed(), &ValueInterpretation::Absent);
    }
    Ok(())
}

// Roche e 411 HIM v2.3 section 6 Tables 17–18.
#[test]
fn dilution_preserves_both_indicators_without_multiplying() -> TestResult {
    for (layout, order, selector, ratio, pre) in [
        (CARET, CARET_PATIENT, "5", "20", "1"),
        (SLASH, SLASH_PATIENT, "20", "20", "pre-diluted"),
    ] {
        let code = if layout == CARET {
            format!("SYN^{selector}^{pre}")
        } else {
            format!("SYN/{selector}/{pre}")
        };
        let output = upload(
            layout,
            order,
            &format!("R|1|^^^{code}|0.12\r"),
            "N",
            Some("numeric"),
        )?;
        let part = first(&output)?;
        let dilution = part.dilution.as_ref().ok_or("dilution")?;
        assert_eq!(
            dilution.original.as_ref().map(SourceText::as_str),
            Some(selector)
        );
        assert_eq!(dilution.factor.as_ref().map(Decimal::as_str), Some(ratio));
        assert_eq!(dilution.applied, DilutionApplied::Unknown);
        assert_eq!(
            part.details
                .as_ref()
                .and_then(|details| details.predilution.as_ref())
                .map(SourceText::as_str),
            Some(pre)
        );
        assert!(
            matches!(part.values.as_slice().first().ok_or("value")?.parsed(), ValueInterpretation::Numeric(n) if n.number.as_str() == "0.12")
        );
    }
    Ok(())
}

// Roche e 411 HIM v2.3 section 6 Tables 19–20.
#[test]
fn result_alarms_keep_the_dialects_own_number_and_comment() -> TestResult {
    for (layout, order, code, comment, description) in [
        (
            CARET,
            CARET_PATIENT,
            "SYN^0^0",
            "42^Synthetic alarm",
            Some("Synthetic alarm"),
        ),
        (SLASH, SLASH_PATIENT, "SYN/1/not", "42", None),
    ] {
        let output = upload(
            layout,
            order,
            &format!("R|1|^^^{code}|0.12\rC|1|I|{comment}|I\r"),
            "N",
            Some("numeric"),
        )?;
        let part = first(&output)?;
        assert_eq!(
            part.comments.as_slice().first().map(SourceText::as_str),
            Some(comment)
        );
        let alarm = part
            .details
            .as_ref()
            .and_then(|details| details.alarms.as_slice().first())
            .ok_or("alarm")?;
        assert_eq!(alarm.number.as_str(), "42");
        assert_eq!(alarm.text.as_ref().map(SourceText::as_str), description);
    }
    Ok(())
}

// Roche e 411 HIM v2.3 section 3 Table 3 and section 6 Table 16.
#[test]
fn generated_and_qc_labels_never_become_patient_ids() -> TestResult {
    for (layout, order, label, control_name, class) in [
        (SLASH, SLASH_PATIENT, "000aBc", None, QcClass::Patient),
        (
            SLASH,
            "O|1|@7|7^A1^1^^S1^SC||||||||N",
            "@7",
            None,
            QcClass::Patient,
        ),
        (
            SLASH,
            "O|1|SyntheticQC|7^A1^1^^QC^SC||||||||Q",
            "SyntheticQC",
            Some("SyntheticQC"),
            QcClass::Control,
        ),
        (
            SLASH_LOT,
            "O|1|SyntheticQC^LOT01|7^A1^1^^QC^SC||||||||Q",
            "SyntheticQC^LOT01",
            None,
            QcClass::Control,
        ),
        (
            SLASH_LOT,
            "O|1|LOT01|7^A1^1^^QC^SC||||||||Q",
            "LOT01",
            None,
            QcClass::Control,
        ),
    ] {
        let output = upload(
            layout,
            order,
            "R|1|^^^SYN/1/not|0.12\r",
            "N",
            Some("numeric"),
        )?;
        let part = first(&output)?;
        let details = part.details.as_ref().ok_or("details")?;
        assert_eq!(
            details.specimen_label.as_ref().map(SourceText::as_str),
            Some(label)
        );
        assert_eq!(
            details.control_name.as_ref().map(SourceText::as_str),
            control_name
        );
        assert_eq!(part.qc_class, class);
        let sample = output
            .results
            .first()
            .ok_or("result")?
            .content
            .sample
            .as_ref();
        assert_eq!(
            sample.map(|sample| sample.id.as_str()),
            (label == "000aBc").then_some(label)
        );
        assert!(output.requires_parking());
        assert!(matches!(
            output.results.first().ok_or("result")?.content.identity,
            IdentityStatus::Unresolved(_)
        ));
    }
    Ok(())
}

fn biolyte_order() -> String {
    let fields: [&str; 26] = std::array::from_fn(|index| match index {
        0 => "O",
        1 => "1",
        3 => "17",
        15 => "Urine",
        25 => "P",
        _ => "",
    });
    fields.join("|")
}

// BioCARE Biolyte Service Manual 4.2A Appendix VI sections 9.4.16, 9.4.26, 10.1.3–4 and p. 86.
#[test]
fn method_specimen_and_report_status_are_independent() -> TestResult {
    for (marker, method) in [
        ("M", MeasurementMethod::Measured),
        ("C", MeasurementMethod::Calculated),
    ] {
        let output = upload(
            MARKER,
            &biolyte_order(),
            &format!("R|1|^^^SYN^{marker}|  1.20 |mmol/L\r"),
            "N",
            None,
        )?;
        let part = first(&output)?;
        assert_eq!(part.test.value.as_str(), "SYN");
        assert_eq!(part.status, ResultStatus::Unknown);
        assert_eq!(part.qc_class, QcClass::Unknown);
        let details = part.details.as_ref().ok_or("details")?;
        assert_eq!(details.method, Some(method));
        assert_eq!(
            details.specimen_descriptor.as_ref().map(SourceText::as_str),
            Some("Urine")
        );
        assert_eq!(details.report_status, Some(ResultStatus::Preliminary));
        let value = part.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), "  1.20 ");
        assert!(
            matches!(value.parsed(), ValueInterpretation::Numeric(n) if n.number.as_str() == "1.20")
        );
    }
    Ok(())
}

// BioCARE Biolyte Service Manual 4.2A Appendix VI section 10.1.4.
#[test]
fn over_width_notation_is_kept_for_review() -> TestResult {
    let output = upload(
        MARKER,
        &biolyte_order(),
        "R|1|^^^SYN^M|+###.##|mmol/L\r",
        "N",
        None,
    )?;
    let value = first(&output)?.values.as_slice().first().ok_or("value")?;
    assert_eq!(value.original.as_str(), "+###.##");
    assert_eq!(value.parsed(), &ValueInterpretation::Text);
    assert!(output.requires_parking());
    Ok(())
}

// BioCARE Biolyte Service Manual 4.2A Appendix VI section 11.1.3.
#[test]
fn sender_abort_holds_retained_measurements() -> TestResult {
    for (code, completion) in [
        ("N", ReportCompletion::Normal),
        ("T", ReportCompletion::SenderAborted),
    ] {
        let output = upload(
            MARKER,
            &biolyte_order(),
            "R|1|^^^SYN^M|1.20|mmol/L\r",
            code,
            None,
        )?;
        let details = first(&output)?.details.as_ref().ok_or("details")?;
        assert_eq!(
            details.termination.as_ref().map(SourceText::as_str),
            Some(code)
        );
        assert_eq!(details.completion, Some(completion));
        assert_eq!(
            output
                .findings
                .iter()
                .any(|finding| finding.problem == Problem::IncompleteAstmReport),
            code == "T"
        );
        // Isolate completion from the synthetic definition's unresolved run identity.
        let mut result = output.results.first().ok_or("result")?.content.clone();
        result.identity = IdentityStatus::Resolved(ResultIdentity {
            observation: ObservationIdentity {
                order: NamespacedId {
                    namespace: Identifier::new("synthetic")?,
                    value: Identifier::new("order")?,
                },
                code: first(&output)?.test.clone(),
                sub_id: SourceText::new("")?,
                run: SourceText::new("run")?,
            },
            revision: Revision(1),
        });
        assert_eq!(result.requires_parking(), code == "T");
    }
    Ok(())
}
