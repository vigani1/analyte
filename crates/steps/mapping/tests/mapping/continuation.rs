//! Refuses incomplete logical messages without changing protected source bytes.
use super::*;
use analyte_fixtures::ConfigEdit as _;

// HL7 2.5.1 section 2.10.2.2; 2.6/2.8.2 section 2.10.2; 2.9.1 section 2.9.2.
#[test]
fn hl7_251_s2_10_2_2_final_fragment_with_msh14_is_never_a_complete_result() -> TestResult {
    for (edition, wire) in [
        ("v251", "2.5.1"),
        ("v26", "2.6"),
        ("v282", "2.8.2"),
        ("v291", "2.9.1"),
    ] {
        let config =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        // Each fragment is independently refused, including after reconnect/restart.
        for (pointer, tail) in [("", "DSC|next\r"), ("next", ""), ("next", "DSC|last\r")] {
            let raw = format!(
                "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}||{pointer}\rOBR|1||000AbC\rOBX|1|NM|SYN^Synthetic^synthetic^^^^v1||0.10||||||F\r{tail}"
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
                Err(analyte_mapping::Error::Structure {
                    reason: "HL7 continuation is unsupported; retain fragment for review",
                    ..
                })
            ));
            assert_eq!(message.encode(), raw.as_bytes());
        }
    }
    Ok(())
}

// HL7 2.3.1 section 7.2.1; 2.5.1 sections 7.3.1 and 7.3.7.
#[test]
fn hl7_231_s7_2_1_251_s7_3_1_s7_3_7_invalid_known_groups_are_refused() -> TestResult {
    let cfg = config(HL7_CONFIG)?;
    for (kind, body) in [
        ("ORU^R01^ORU_R01", "ORC|RE\r"),
        ("ORU^R01^ORU_R01", "ORC|RE\rORC|RE\rOBR|1\r"),
        ("ORU^R01^ORU_R01", "PID|1\rPV2|1\rOBR|1\r"),
        ("OUL^R22^OUL_R22", "PID|1\rPID|2\rSPM|1\rOBR|1\r"),
        ("OUL^R22^OUL_R22", "SPM|1\rSPM|2\rOBR|1\r"),
    ] {
        let raw = format!("MSH|^~\\&|SYN||||20261116||{kind}|MSG|P|2.5.1\r{body}");
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            cfg.effective().hl7_options().ok_or("options")?,
        )?;
        assert!(
            map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&cfg).0,
                    mappings: settings::hl7(&cfg).1,
                    declarations: &cfg.effective().machine().deviations
                },
                &message,
                input()
            )
            .is_err()
        );
        assert_eq!(message.encode(), raw.as_bytes());
    }
    Ok(())
}

// HL7 2.5.1 section 7.4.3.17: DR endpoints contain independent TS precision.
#[test]
fn hl7_251_s7_4_3_17_collection_range_is_not_ts_precision() -> TestResult {
    let cfg = config(HL7_CONFIG)?;
    for range in ["20261116^20261117", "20261116&D", "20261116&D^20261117&D"] {
        let mut spm = [""; 17];
        spm[0] = "1";
        spm[16] = range;
        let raw = format!(
            "MSH|^~\\&|SYN||||20261116||OUL^R22^OUL_R22|MSG|P|2.5.1\rSPM|{}\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r",
            spm.join("|")
        );
        let message = analyte_hl7::Message::parse(
            raw.as_bytes(),
            cfg.effective().hl7_options().ok_or("options")?,
        )?;
        assert!(matches!(
            map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&cfg).0,
                    mappings: settings::hl7(&cfg).1,
                    declarations: &cfg.effective().machine().deviations
                },
                &message,
                input()
            ),
            Err(analyte_mapping::Error::Structure {
                reason: "SPM-17 collection range or nested precision is unsupported; retain interval for review",
                ..
            })
        ));
        assert_eq!(message.encode(), raw.as_bytes());
    }
    Ok(())
}

// HL7 2.3.1 section 7.3.2.11; 2.5.1+ sections 7.4.2.11 and 7.4.1.25;
// 2.8.2/2.9.1 section 7.4.1.22 requires report time when report status is populated.
#[test]
fn hl7_s7_4_2_11_s7_4_1_25_missing_or_unknown_report_context_parks() -> TestResult {
    for (edition, wire) in [
        ("v231", "2.3.1"),
        ("v251", "2.5.1"),
        ("v26", "2.6"),
        ("v282", "2.8.2"),
        ("v291", "2.9.1"),
    ] {
        let cfg =
            config(&HL7_CONFIG.rewrite("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
        for (segment, field, value) in [
            ("OBX", 11, "F"),
            ("OBX", 11, ""),
            ("OBX", 11, "\"\""),
            ("OBX", 11, "UNKNOWN"),
            ("OBR", 4, ""),
            ("OBR", 25, ""),
            ("OBR", 25, "\"\""),
            ("OBR", 25, "UNKNOWN"),
            ("OBR", 22, ""),
        ] {
            let mut obr = [""; 25];
            obr[0] = "1";
            obr[2] = "000AbC";
            obr[3] = "SYN^Synthetic^synthetic^^^^v1";
            obr[21] = "20261116120000";
            obr[24] = "F";
            let mut obx = [""; 11];
            obx[0] = "1";
            obx[1] = "NM";
            obx[2] = "SYN^Synthetic^synthetic^^^^v1";
            obx[4] = "1";
            obx[10] = "F";
            if segment == "OBR" {
                obr[field - 1] = value;
            } else {
                obx[field - 1] = value;
            }
            let raw = format!(
                "MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|{wire}\rOBR|{}\rOBX|{}\r",
                obr.join("|"),
                obx.join("|")
            );
            let message = analyte_hl7::Message::parse(
                raw.as_bytes(),
                cfg.effective().hl7_options().ok_or("options")?,
            )?;
            let output = map_hl7(
                &analyte_mapping::hl7::Settings {
                    syntax: settings::hl7(&cfg).0,
                    mappings: settings::hl7(&cfg).1,
                    declarations: &cfg.effective().machine().deviations,
                },
                &message,
                input(),
            )?;
            let must_park = value != "F" && (field != 22 || matches!(edition, "v282" | "v291"));
            assert_eq!(
                output.requires_parking(),
                must_park,
                "{edition} {segment}-{field} {value}"
            );
            assert_eq!(message.encode(), raw.as_bytes());
        }
    }
    Ok(())
}

// HL7 2.5.1 section 2.10.1: a sequence number does not negotiate a sequence protocol.
#[test]
fn hl7_251_s2_10_1_unnegotiated_sequence_number_is_refused() -> TestResult {
    let cfg = config(HL7_CONFIG)?;
    let raw=b"MSH|^~\\&|SYN||||20261116||ORU^R01^ORU_R01|MSG|P|2.5.1|1\rOBR|1||001\rOBX|1|NM|SYN^^synthetic||1||||||F\r";
    let message =
        analyte_hl7::Message::parse(raw, cfg.effective().hl7_options().ok_or("options")?)?;
    assert!(matches!(
        map_hl7(
            &analyte_mapping::hl7::Settings {
                syntax: settings::hl7(&cfg).0,
                mappings: settings::hl7(&cfg).1,
                declarations: &cfg.effective().machine().deviations
            },
            &message,
            input()
        ),
        Err(analyte_mapping::Error::Structure {
            reason: "HL7 sequence-number negotiation is unsupported; retain receipt for review",
            ..
        })
    ));
    assert_eq!(message.encode(), raw);
    Ok(())
}
