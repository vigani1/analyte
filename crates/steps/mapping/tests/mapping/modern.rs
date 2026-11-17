//! Tests bounded modern result delivery without inventing extension meanings.
use super::*;
use analyte_fixtures::ConfigEdit as _;

#[test]
fn hl7_2_8_2_s7_3_1_and_2_9_1_s7_3_1_modern_incomplete_reports_keep_exact_values_parked()
-> TestResult {
    for (edition, wire) in [("v282", "2.8.2"), ("v291", "2.9.1")] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        for (kind, value, decoded) in [
            ("NM", "0.10", None),
            ("ST", "literal\\P\\", Some("literal#")),
            ("CWE", "POS^Positive^LOCAL^^^^v1^^^ALT^^OTHER^v2", None),
            ("DTM", "20261116123045.1234+0000", None),
        ] {
            let raw = format!(
                "MSH|^~\\&#|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|1||000AbC\rOBX|1|{kind}|SYN^Synthetic^synthetic^^^^v1||{value}||||||F\r"
            );
            let message = analyte_hl7::Message::parse(
                raw.as_bytes(),
                config.effective().hl7_options().ok_or("options")?,
            )?;
            let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
            assert!(output.requires_parking(), "{output:?}");
            let value_out = first(&output)?.values.as_slice().first().ok_or("value")?;
            assert_eq!(value_out.original.as_str(), value);
            if let Some(decoded) = decoded {
                assert_eq!(value_out.decoded_text().ok_or("decoded")?.as_str(), decoded);
            }
            assert_eq!(message.encode(), raw.as_bytes());
        }
    }
    Ok(())
}

#[test]
fn hl7_2_9_1_s2a_2_22_utc_without_known_civil_offset_remains_raw() -> TestResult {
    for (edition, wire) in [("v282", "2.8.2"), ("v291", "2.9.1")] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        for zone in ["+0000", "-0000"] {
            let date = format!("20261116123045{zone}");
            let mut fields = [""; 19];
            fields[0] = "1";
            fields[1] = "NM";
            fields[2] = "SYN^Synthetic^synthetic^^^^v1";
            fields[4] = "0.10";
            fields[10] = "F";
            fields[18] = &date;
            let raw = format!(
                "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|1||000AbC\rOBX|{}\r",
                fields.join("|")
            );
            let message = analyte_hl7::Message::parse(
                raw.as_bytes(),
                config.effective().hl7_options().ok_or("options")?,
            )?;
            let output = map_hl7(&settings::hl7_mapping(&config), &message, input())?;
            let time = first(&output)?
                .times
                .as_slice()
                .iter()
                .find(|time| time.role.as_str() == "OBX.19")
                .ok_or("analysis time")?;
            assert_eq!(time.time.original.as_str(), date);
            let interpreted = edition != "v291" || zone != "-0000";
            assert_eq!(time.time.parsed.is_some(), interpreted);
            assert_eq!(time.time.offset.is_some(), interpreted);
            assert!(output.requires_parking(), "{output:?}");
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_and_2_9_1_s7_3_1_unselected_groups_and_withdrawn_fields_do_not_map() -> TestResult {
    for (edition, wire) in [("v282", "2.8.2"), ("v291", "2.9.1")] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        for extra in [
            "SPM|1|source\r",
            "SAC|container\r",
            "PRT|participation\r",
            "ARV|restriction\r",
            "DEV|device\r",
            "SFT|software\r",
        ] {
            let raw = format!(
                "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|1||000AbC\r{extra}OBX|1|NM|SYN^Synthetic^synthetic^^^^v1||0.10||||||F\r"
            );
            let message = analyte_hl7::Message::parse(
                raw.as_bytes(),
                config.effective().hl7_options().ok_or("options")?,
            )?;
            assert!(
                map_hl7(
                    &analyte_mapping::hl7::Settings {
                        syntax: settings::hl7(&config).0,
                        mappings: settings::hl7(&config).1,
                        declarations: &config.effective().machine().deviations
                    },
                    &message,
                    input()
                )
                .is_err()
            );
            assert_eq!(message.raw(), raw.as_bytes());
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_5_5_2_and_2_9_1_s2_4_5_2_truncated_results_remain_parked() -> TestResult {
    for (edition, wire) in [("v282", "2.8.2"), ("v291", "2.9.1")] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let raw = format!(
            "MSH|^~\\&#|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|1||000AbC\rOBX|1|ST|SYN^Synthetic^synthetic^^^^v1||incomplete#||||||F\r"
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            config.effective().hl7_options().ok_or("options")?,
        )?;
        assert!(
            map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&config).0,
                    mappings: settings::hl7(&config).1,
                    declarations: &config.effective().machine().deviations
                },
                &message,
                input()
            )
            .is_err()
        );
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_a_13_and_2_9_1_s2a_2_13_oid_authority_never_uses_code_only_fallback() -> TestResult
{
    for (edition, wire) in [("v282", "2.8.2"), ("v291", "2.9.1")] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        let mut code = [""; 14];
        code[0] = "SYN"; // Deliberately matches a configured code from a different named namespace.
        code[13] = "1.2.3";
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|1||000AbC\rOBX|1|NM|{}||0.10||||||F\r",
            code.join("^")
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            config.effective().hl7_options().ok_or("options")?,
        )?;
        assert!(matches!(
            map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&config).0,
                    mappings: settings::hl7(&config).1,
                    declarations: &config.effective().machine().deviations
                },
                &message,
                input()
            ),
            Err(analyte_mapping::Error::Hl7 {
                source: analyte_hl7::Error::Control {
                    segment: "OBX",
                    field: 3
                },
                ..
            })
        ));
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}
