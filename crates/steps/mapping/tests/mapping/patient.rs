//! Qualifies patient-related new work without blocking an existing order cancellation.

use analyte_fixtures::ConfigEdit as _;
// HL7 2.3.1 section 4.2.1.1(d) applies required PID to new patient orders.
#[test]
fn hl7_231_s4_2_1_1_d_patient_requiredness_distinguishes_new_and_cancel()
-> Result<(), Box<dyn std::error::Error>> {
    let definition = format!(
        "{}\n{}",
        include_str!("../../../../core/config/examples/hl7.toml")
            .rewrite("edition = \"v251\"", "edition = \"v231\""),
        include_str!("../../../../core/config/examples/hl7_send.toml")
    );
    let config = analyte_config::Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()?;
    for (state, allowed) in [("requested", false), ("cancel_requested", true)] {
        let document = format!(
            r#"{{"id":"order-1","version":1,"state":"{state}","sample_id":"001","tests":[{{"namespace":"synthetic","value":"TEST"}}],"priority":"routine","tombstone":false,"patient":{{"id":{{"namespace":"patients","value":"P1"}}}}}}"#
        );
        let app = analyte_contract::decode(document.as_bytes())?;
        assert_eq!(
            analyte_mapping::map_hl7_order(
                crate::settings::hl7(&config).0,
                crate::settings::orders(&config),
                &app,
                analyte_hl7::Options::default()
            )
            .is_ok(),
            allowed
        );
    }
    Ok(())
}
