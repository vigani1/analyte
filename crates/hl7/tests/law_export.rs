//! IHE `PaLM` TF-2b Appendix C: original synthetic export regression.
#[path = "../examples/law_messages.rs"]
mod law_messages;

#[test]
fn law_tf2b_appendix_c_export_messages_reparse() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::env::temp_dir().join(format!("analyte-law-export-test-{}", std::process::id()));
    std::fs::create_dir(&directory)?;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        law_messages::generate(&directory)?;
        let names = [
            "rsp_k11_found.hl7",
            "rsp_k11_empty.hl7",
            "oml_o33_new.hl7",
            "oml_o33_cancel.hl7",
            "oml_o33_negative.hl7",
            "ack_r22_aa.hl7",
            "ack_r22_ae.hl7",
            "qbp_q11_specimen.hl7",
            "qbp_q11_all_pending.hl7",
            "orl_o34_accept.hl7",
            "oul_r22.hl7",
        ];
        assert_eq!(std::fs::read_dir(&directory)?.count(), names.len());
        for name in names {
            let bytes = std::fs::read(directory.join(name))?;
            assert!(bytes.starts_with(b"MSH|"), "{name}");
            assert!(bytes.ends_with(b"\r"), "{name}");
            assert!(
                !bytes.contains(&b'\n') && !bytes.contains(&0x0b) && !bytes.contains(&0x1c),
                "{name}"
            );
            let message = analyte_hl7::law::parse_message(
                &bytes,
                analyte_hl7::Options {
                    charset: analyte_hl7::Charset::Utf8,
                    ..analyte_hl7::Options::default()
                },
            )?;
            analyte_hl7::law::validate_control(
                &message,
                analyte_hl7::law::ControlDirection::Receive,
            )?;
            assert_eq!(message.raw(), bytes);
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&directory)?;
    outcome
}
