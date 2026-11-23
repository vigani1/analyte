//! Exercises the documented categorized ORU layouts through validated machine settings.
use super::*;
use analyte_fixtures::ConfigEdit as _;

fn definition(layout: bool) -> String {
    let category = include_str!("../../../../../core/config/tests/fixtures/departure.toml")
        .rewrite("astm_frame_limit", "hl7_msh16_message_category")
        .rewrite("compatibility", "msh16_message_category")
        .rewrite("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v231\"")
        .rewrite("astm_results", "oru_r01");
    let category = if layout {
        let declaration = category
            .lines()
            .find_map(|line| {
                line.strip_prefix("deviations = [")
                    .and_then(|line| line.strip_suffix(']'))
            })
            .unwrap_or_default();
        let extra = declaration
            .rewrite("hl7_msh16_message_category", "hl7_categorized_oru_results")
            .rewrite("msh16_message_category", "categorized_oru_results");
        category.rewrite(declaration, &format!("{declaration}, {extra}"))
    } else {
        category
    };
    HL7_CONFIG
        .rewrite("deviations = []", &category)
        .rewrite("edition = \"v251\"", "edition = \"v231\"")
        .rewrite("field = 3", "field = 2")
}

fn map(category: u8, body: &str, layout: bool) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(&definition(layout))?;
    let raw = format!("MSH|^~\\&|SYN||||202611231255||ORU^R01|MSG|P|2.3.1||||{category}\r{body}");
    let mut options = effective_options(config.effective()).ok_or("missing options")?;
    options.control = analyte_hl7::ControlProfile::Msh16MessageCategory;
    let message = analyte_hl7::Message::parse(raw.as_bytes(), options)?;
    Ok(map_hl7(&settings::hl7_mapping(&config), &message, input())?)
}

fn patient(obx: &str) -> String {
    format!("PID|1\rOBR|1|000aB|37|PANEL|||||2.00|r02^004\r{obx}\r")
}

// YHLO LIS V1.1 sections 2.3.4.1 and 2.3.5; service manual section 6.4.2.1.
#[test]
fn barcode_rack_dilution_and_status_have_separate_source_meanings() -> TestResult {
    let output = map(0, &patient("OBX|1|NM|A7|1|00.20|mg/L|||||F||00.10"), true)?;
    let result = output.results.first().ok_or("missing observation")?;
    let sample = result.content.sample.as_ref().ok_or("missing sample")?;
    assert_eq!(sample.id.as_str(), "000aB");
    let rack = sample.position.as_ref().ok_or("missing position")?;
    assert_eq!(rack.carrier.as_ref().ok_or("missing rack")?.as_str(), "r02");
    assert_eq!(rack.slot.as_str(), "004");
    let part = first(&output)?;
    let dilution = part.dilution.as_ref().ok_or("missing dilution")?;
    assert_eq!(
        dilution
            .original
            .as_ref()
            .ok_or("missing factor text")?
            .as_str(),
        "2.00"
    );
    assert_eq!(dilution.applied, DilutionApplied::Unknown);
    assert_eq!(
        part.raw_measurement
            .as_ref()
            .ok_or("missing raw measurement")?
            .as_str(),
        "00.10"
    );
    assert_eq!(part.source_status.as_str(), "F");
    assert_eq!(part.status, ResultStatus::Final);
    assert_eq!(part.qc_class, QcClass::Patient);
    assert!(matches!(
        result.content.identity,
        IdentityStatus::Unresolved(_)
    ));
    assert!(output.requires_parking());
    let partial = map(0, &patient("OBX|1|NM|A7|1|0.2||||||P"), true)?;
    assert!(partial.requires_parking());
    assert_eq!(first(&partial)?.source_status.as_str(), "P");
    assert_eq!(first(&partial)?.status, ResultStatus::Unknown);
    Ok(())
}

// YHLO LIS V1.1 sections 2.3.5 and 2.4.1.1; HL7 2.3.1 section 7.3.2.2.
#[test]
fn both_retains_quantity_and_call_as_values_of_one_observation() -> TestResult {
    let body = patient("OBX|1|BOTH|A7|1|00.20|mg/L|0.1-0.3||+-|neg|F");
    let output = map(0, &body, true)?;
    let part = first(&output)?;
    let [quantity, call] = part.values.as_slice() else {
        return Err("expected two values".into());
    };
    assert_eq!(quantity.original.as_str(), "00.20");
    assert!(matches!(quantity.parsed(), ValueInterpretation::Numeric(_)));
    assert_eq!(call.original.as_str(), "+-");
    assert_eq!(
        call.reference
            .as_ref()
            .ok_or("missing qualitative reference")?
            .as_str(),
        "neg"
    );
    assert_eq!(
        part.reference_range
            .as_ref()
            .ok_or("missing numeric range")?
            .as_str(),
        "0.1-0.3"
    );
    let record = output.results.first().ok_or("missing observation")?;
    let document = analyte_contract::ResultDocument::from(&record.content);
    let [document_part] = document.parts.as_slice() else {
        return Err("expected one contract part".into());
    };
    let [document_quantity, document_call] = document_part.values.as_slice() else {
        return Err("expected two contract values".into());
    };
    assert_eq!(document_quantity.original.as_str(), "00.20");
    assert_eq!(document_call.original.as_str(), "+-");
    assert_eq!(
        document_call
            .reference
            .as_ref()
            .ok_or("missing contract reference")?
            .as_str(),
        "neg"
    );
    assert_eq!(output.results.len(), 1);
    assert_eq!(output.applied_departures.len(), 1);
    assert!(
        map(0, &body, false)?
            .findings
            .iter()
            .any(|f| f.problem == Problem::UnsupportedValue)
    );
    Ok(())
}

// YHLO LIS V1.1 sections 2.3.5 and 2.4.1.1.
#[test]
fn empty_primary_preserves_the_reported_qualitative_call() -> TestResult {
    for datatype in ["ST", "BOTH"] {
        let body = patient(&format!("OBX|1|{datatype}|A7|1|||||-|neg|F"));
        let output = map(0, &body, true)?;
        let [call] = first(&output)?.values.as_slice() else {
            return Err("expected one value".into());
        };
        assert_eq!(call.original.as_str(), "-");
        assert_eq!(
            call.reference
                .as_ref()
                .ok_or("missing qualitative reference")?
                .as_str(),
            "neg"
        );
    }
    Ok(())
}

// YHLO LIS V1.1 section 2.3.5.
#[test]
fn interference_parts_keep_their_matching_reported_and_raw_values() -> TestResult {
    let output = map(
        0,
        &patient("OBX|1|NM|A7|1|1.00^2.00^3.00||||||F||11^22^33"),
        true,
    )?;
    let result = output.results.first().ok_or("missing panel")?;
    let parts = result.content.parts.as_slice();
    assert_eq!(parts.len(), 3);
    for (part, (component, reported, raw)) in parts.iter().zip([
        (InterferenceComponent::Turbidity, "1.00", "11"),
        (InterferenceComponent::Hemolysis, "2.00", "22"),
        (InterferenceComponent::Jaundice, "3.00", "33"),
    ]) {
        assert_eq!(part.interference.as_ref(), Some(&component));
        let [value] = part.values.as_slice() else {
            return Err("expected one value".into());
        };
        assert_eq!(value.original.as_str(), reported);
        assert_eq!(
            part.raw_measurement
                .as_ref()
                .ok_or("missing raw value")?
                .as_str(),
            raw
        );
    }
    for raw in ["11^33", "11^^33", "11^22^33~44", ""] {
        let body = patient(&format!("OBX|1|NM|A7|1|1^2^3||||||F||{raw}"));
        assert!(map(0, &body, true).is_err());
    }
    for body in [
        patient("OBX|1|NM|A7|1|1^2^3|unit|||||F||11^22^33"),
        patient("OBX|1|ST|A7|1|1^2^3||||||F||11^22^33"),
    ] {
        assert!(map(0, &body, true).is_err());
    }
    Ok(())
}

const QC: &str = "OBR|1|B9|B9|PANEL|||202611231250||||2|x^y|alpha^beta|L001^L009|202711^202811||L^H|1.0^2.0|0.1^0.2|01.10^02.20\r";

// YHLO LIS V1.1 sections 2.3.1, 2.3.4.2 and 2.4.1.2.
#[test]
fn qc_counts_and_columns_keep_each_lot_and_level_with_its_value() -> TestResult {
    let output = map(2, QC, true)?;
    let result = output.results.first().ok_or("missing QC report")?;
    assert!(result.content.sample.is_none());
    assert!(output.requires_parking());
    assert_eq!(result.content.parts.as_slice().len(), 2);
    for (part, (lot, level, measured)) in result
        .content
        .parts
        .as_slice()
        .iter()
        .zip([("L001", "L", "01.10"), ("L009", "H", "02.20")])
    {
        assert_eq!(part.test.value.as_str(), "B9");
        assert_eq!(part.qc_class, QcClass::Control);
        let metadata = part.control.as_ref().ok_or("missing matched QC metadata")?;
        assert_eq!(metadata.lot.as_str(), lot);
        assert_eq!(metadata.level.as_str(), level);
        let [value] = part.values.as_slice() else {
            return Err("expected a single reading".into());
        };
        assert_eq!(value.original.as_str(), measured);
    }
    for (old, new) in [
        ("L001^L009", "L001^^L009"),
        ("L^H", "^H"),
        ("|2|x^y", "|3|x^y"),
        ("L001^L009", "L001~L009"),
        ("B9|B9", "B9|B8"),
        ("01.10^02.20", "1"),
    ] {
        assert!(map(2, &QC.replace(old, new), true).is_err());
    }
    assert!(map(2, &format!("{QC}OBX|1|NM|B9|1|03\r"), true).is_err());
    assert!(map(2, QC, false).is_err());
    assert!(map(1, QC, true).is_err());
    Ok(())
}

// ADR 0015 and YHLO LIS V1.1 section 2.3.1.
#[test]
fn categorized_layout_requires_its_header_departure_and_evidence() {
    let enabled = definition(true);
    let category = enabled.find(", { rule = \"hl7_categorized_oru_results\"");
    assert!(category.is_some());
    if let Some(category) = category {
        let start = enabled.find("deviations = [").unwrap_or_default();
        let mut invalid = enabled.clone();
        invalid.replace_range(start + "deviations = [".len()..category + 2, "");
        assert!(config(&invalid).is_err());
    }
    assert!(
        config(&enabled.replace(
            "guarantee=\"categorized_oru_results\"",
            "guarantee=\"compatibility\""
        ))
        .is_err()
    );
}
