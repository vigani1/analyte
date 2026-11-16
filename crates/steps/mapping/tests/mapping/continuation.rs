//! Refuses incomplete logical messages without changing protected source bytes.
use super::*;

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
            config(&HL7_CONFIG.replace("edition = \"v251\"", &format!("edition = \"{edition}\"")))?;
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
                map_hl7(&message, &config, input()),
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
