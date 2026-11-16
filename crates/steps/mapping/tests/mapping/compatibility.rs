//! Tests legacy result interpretation without assigning uncertain extension meanings.
use super::*;

#[test]
fn law_tf2b_c4_and_3_29_4_compatible_vid_keeps_law_analysis_time() -> TestResult {
    let definition = include_str!("../../../../core/config/examples/law.toml").replace(
        "order_namespace = {field={group=\"order\",record=\"OBR\",field=2,component=2,repetition=1}}",
        "order_namespace = {fixed=\"manager\"}",
    );
    let config = config(&definition)?;
    for version in ["2.5", "2.5.1", "2.5.2"] {
        let raw = std::str::from_utf8(LAW_RESULT)?.replace("|2.5.1|", &format!("|{version}|"));
        let message = analyte_hl7::law::parse_message(
            raw.as_bytes(),
            config.effective().hl7_options().ok_or("options")?,
        )?;
        let results = analyte_hl7::law::Results::new(&message)?;
        let output = analyte_mapping::map_law(&results, &config, input())?;
        assert!(!output.requires_parking(), "{output:?}");
        let time = first(&output)?
            .times
            .as_slice()
            .iter()
            .find(|time| time.role.as_str() == "OBX.19")
            .ok_or("LAW analysis time was lost")?;
        assert_eq!(time.time.original.as_str(), "20261115120000");
        assert!(time.time.parsed.is_some());
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn hl7_2_5_1_s2_8_1_legacy_extension_groups_are_not_assigned_later_specimen_meanings() -> TestResult
{
    for (edition, version) in [("v23", "2.3"), ("v24", "2.4"), ("v25", "2.5")] {
        let config =
            config(&HL7_CONFIG.replace("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{version}\rOBR|1||000AbC\rSPM|1|opaque\rOBX|1|NM|SYN||0.10||||||F\r"
        );
        let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
        assert!(matches!(
            map_hl7(&message, &config, input()),
            Err(analyte_mapping::Error::Structure {
                position: analyte_mapping::Position {
                    record: 3,
                    field: 0
                },
                ..
            })
        ));
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn hl7_2_5_1_s2_8_1_2_legacy_results_map_stable_fields_and_keep_uncertain_extensions_raw()
-> TestResult {
    for (edition, version) in [("v23", "2.3"), ("v24", "2.4"), ("v25", "2.5")] {
        let config =
            config(&HL7_CONFIG.replace("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let mut fields = vec![""; 19];
        for (index, value) in [
            (0, "1"),
            (1, "NM"),
            (2, "SYN^Synthetic^synthetic"),
            (4, "0.10"),
            (5, "mg/L"),
            (10, "F"),
            (18, "202611161215"),
        ] {
            *fields.get_mut(index).ok_or("synthetic field index")? = value;
        }
        let mut obr = vec![""; 35];
        *obr.get_mut(2).ok_or("sample field")? = "000AbC";
        *obr.get_mut(31).ok_or("interpreter field")? = "opaque&name";
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{version}\rOBR|{}\rOBX|{}\r",
            obr.join("|"),
            fields.join("|")
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            config.effective().hl7_options().ok_or("options")?,
        )?;
        let output = map_hl7(&message, &config, input())?;
        let part = first(&output)?;
        assert_eq!(
            part.values
                .as_slice()
                .first()
                .ok_or("value")?
                .original
                .as_str(),
            "0.10"
        );
        assert!(
            part.times
                .as_slice()
                .iter()
                .all(|time| time.role.as_str() != "OBX.19")
        );
        assert_eq!(message.encode(), raw.as_bytes());
        let obr = message
            .segments()
            .find(|segment| segment.id() == b"OBR")
            .ok_or("OBR")?;
        assert_eq!(obr.field(3).ok_or("sample")?.raw(), b"000AbC");
        assert_eq!(obr.field(32).ok_or("interpreter")?.raw(), b"opaque&name");
    }
    Ok(())
}

#[test]
fn hl7_2_5_1_s2_8_1_new_datatype_availability_is_not_guessed_for_legacy_cwe() -> TestResult {
    for (edition, version) in [("v23", "2.3"), ("v24", "2.4"), ("v25", "2.5")] {
        let config =
            config(&HL7_CONFIG.replace("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{version}\rOBR|1||000AbC\rOBX|1|CWE|SYN^Synthetic^synthetic||code^text^local||||||F\r"
        );
        let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
        let output = map_hl7(&message, &config, input())?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), "code^text^local");
        assert!(value.parsed.is_unsupported());
        assert!(
            output
                .results
                .iter()
                .all(|result| result.content.requires_parking())
        );
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}
