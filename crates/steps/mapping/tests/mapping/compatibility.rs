//! Tests legacy result interpretation without assigning uncertain extension meanings.
use super::*;
use analyte_fixtures::ConfigEdit as _;

#[test]
fn hl7_2_6_s7_3_1_s7_4_2_3_5_19_21_receiving_keeps_values_times_and_raw_identity() -> TestResult {
    let config = config(&HL7_CONFIG.rewrite("edition = \"v251\"", "edition = \"v26\""))?;
    for (datatype, value) in [
        ("NM", "0.10"),
        ("CWE", "POS^Positive^LOCAL^^^^v1"),
        ("DTM", "20261116123045.1200-0530"),
    ] {
        let mut fields = [""; 21];
        fields[0] = "1";
        fields[1] = datatype;
        fields[2] = "SYN^Synthetic^synthetic^^^^v1";
        fields[4] = value;
        fields[10] = "F";
        fields[18] = "20261116123045.1200-0530";
        fields[20] = "opaque-instance^external";
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|2.6\rOBR|1||000AbC\rOBX|{}\r",
            fields.join("|")
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
        assert!(output.requires_parking(), "{output:?}");
        let part = first(&output)?;
        assert_eq!(
            part.values
                .as_slice()
                .first()
                .ok_or("value")?
                .original
                .as_str(),
            value
        );
        assert!(
            part.times
                .as_slice()
                .iter()
                .any(|time| time.role.as_str() == "OBX.19"
                    && time.time.original.as_str() == "20261116123045.1200-0530")
        );
        assert_eq!(message.encode(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn hl7_2_6_s2_a_6_77_and_s7_4_2_22_unsafe_results_never_map() -> TestResult {
    let config = config(&HL7_CONFIG.rewrite("edition = \"v251\"", "edition = \"v26\""))?;
    for observation in [
        "OBX|1|CE|SYN||POS||||||F\r",
        "OBX|1|TS|SYN||20261116||||||F\r",
        "OBX|1|CWE|SYN||POS^Positive^LOCAL||||||F\r",
        "OBX|1|NM|SYN||0.10||||||F|||||||||||INT^Intent^HL70725\r",
        "SPM|1|opaque\rOBX|1|NM|SYN||0.10||||||F\r",
    ] {
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|2.6\rOBR|1||000AbC\r{observation}"
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            effective_options(config.effective()).ok_or("options")?,
        )?;
        assert!(map_hl7(&settings::hl7_mapping(&config), &message, input()).is_err());
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_and_3_29_4_compatible_vid_keeps_law_analysis_time() -> TestResult {
    let definition = include_str!("../../../../core/config/examples/law.toml").rewrite(
        "order_namespace = {fixed=\"LAB\"}",
        "order_namespace = {fixed=\"manager\"}",
    );
    let config = config(&definition)?;
    for version in ["2.5", "2.5.1", "2.5.2"] {
        let raw = std::str::from_utf8(LAW_RESULT)?.rewrite("|2.5.1|", &format!("|{version}|"));
        let message = analyte_hl7::law::parse_message(
            raw.as_bytes(),
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let results = analyte_hl7::law::Results::new(&message)?;
        let output = analyte_mapping::law::map(settings::law(&config), &results, input())?;
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
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{version}\rOBR|1||000AbC\rSPM|1|opaque\rOBX|1|NM|SYN||0.10||||||F\r"
        );
        let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
        assert!(matches!(
            map_hl7(&settings::hl7_mapping(&config), &message, input()),
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
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
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
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
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
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{version}\rOBR|1||000AbC\rOBX|1|CWE|SYN^Synthetic^synthetic||code^text^local||||||F\r"
        );
        let message = analyte_hl7::Message::parse(raw.as_bytes(), analyte_hl7::Options::default())?;
        let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
        let value = first(&output)?.values.as_slice().first().ok_or("value")?;
        assert_eq!(value.original.as_str(), "code^text^local");
        assert!(value.parsed().is_unsupported());
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

// HL7 2.3.1 sections 7.2.1 and 7.3.2: no standard SPM/SAC or OBX-19 analysis time.
#[test]
fn hl7_231_s7_2_1_s7_3_2_later_fields_do_not_gain_meaning() -> TestResult {
    let config = config(&HL7_CONFIG.rewrite("edition = \"v251\"", "edition = \"v231\""))?;
    for extra in ["", "SPM|1|later-specimen\r", "SAC|||later-container\r"] {
        let mut fields = [""; 19];
        fields[0] = "1";
        fields[1] = "NM";
        fields[2] = "SYN^^synthetic";
        fields[4] = "1";
        fields[10] = "F";
        fields[18] = "20261116123045";
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01|MSG|P|2.3.1\r{extra}OBR|1||000AbC\rOBX|{}\r",
            fields.join("|")
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            effective_options(config.effective()).ok_or("options")?,
        )?;
        let result = map_hl7(&settings::hl7_mapping(&config), &message, input());
        if extra.is_empty() {
            assert!(
                !first(&result?)?
                    .times
                    .as_slice()
                    .iter()
                    .any(|time| time.role.as_str() == "OBX.19")
            );
        } else {
            assert!(result.is_err());
        }
        assert_eq!(message.encode(), raw.as_bytes());
    }
    Ok(())
}
