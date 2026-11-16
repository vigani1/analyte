//! Tests ADR 0015 missing-status interpretations against HL7 §§7.4.1.25 and 7.4.2.11.
use super::*;
fn definition(rule: &str, guarantee: &str) -> String {
    let fixture = include_str!("../../../../core/config/tests/fixtures/departure.toml")
        .replace("astm_frame_limit", rule)
        .replace("compatibility", guarantee)
        .replace("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v251\"")
        .replace("astm_results", "oru_r01_patient");
    HL7_CONFIG.replace("deviations = []",&fixture).replace("qc = []", "qc = [{field={group='order',record='OBR',field=3,component=1,repetition=1},value='000AbC',classification='patient'}]")
}
fn report(
    report_status: &str,
    observation_status: &str,
    definition: &str,
) -> Result<Output, Box<dyn std::error::Error>> {
    let mut obr = [""; 25];
    obr[0] = "1";
    obr[2] = "000AbC";
    obr[3] = "SYN^Synthetic^synthetic";
    obr[24] = report_status;
    let mut obx = [""; 11];
    obx[0] = "1";
    obx[1] = "NM";
    obx[2] = "SYN^Synthetic^synthetic";
    obx[4] = "1";
    obx[10] = observation_status;
    hl7(
        &format!("OBR|{}\rOBX|{}\r", obr.join("|"), obx.join("|")),
        definition,
    )
}
#[test]
fn adr0015_hl7_s74125_only_qualified_absence_is_discharged_and_v1_parks() -> TestResult {
    let definition = definition("hl7_absent_obr25_means_final", "absent_obr25_final");
    let mut mapped = report("", "F", &definition)?;
    assert_eq!(mapped.applied_departures.len(), 1);
    assert!(
        mapped
            .findings
            .iter()
            .any(|f| f.problem == Problem::DepartureDerivedStatusRequiresContract)
    );
    assert!(
        !mapped
            .findings
            .iter()
            .any(|f| f.problem == Problem::UnqualifiedHl7Report && f.position.field == 25)
    );
    assert!(mapped.requires_parking());
    assert_eq!(first(&mapped)?.source_status.as_str(), "F");
    mapped.findings.clear();
    assert!(
        mapped.requires_parking(),
        "derived status remains ineligible even without diagnostic display"
    );
    for value in ["\"\"", "UNKNOWN", "P", "C", "R", "X", "D"] {
        let mapped = report(value, "F", &definition)?;
        assert!(mapped.applied_departures.is_empty());
        assert!(mapped.requires_parking());
    }
    let received = report("F", "F", &definition)?;
    assert!(received.applied_departures.is_empty());
    assert!(!received.requires_parking());
    Ok(())
}
#[test]
fn adr0015_hl7_s74211_derived_summary_preserves_absence_and_identity_units() -> TestResult {
    let definition = definition("hl7_absent_obx11_means_final", "absent_obx11_final");
    let mapped = report("F", "", &definition)?;
    assert_eq!(first(&mapped)?.source_status.as_str(), "");
    assert_eq!(first(&mapped)?.status, ResultStatus::Final);
    assert!(first(&mapped)?.units.is_none());
    assert_eq!(mapped.applied_departures.len(), 1);
    assert!(mapped.requires_parking());
    let both = report("", "", &definition)?;
    assert!(
        both.findings
            .iter()
            .any(|f| f.problem == Problem::UnqualifiedHl7Report && f.position.field == 25)
    );
    let qc = definition.replace("classification='patient'", "classification='control'");
    assert!(report("F", "", &qc)?.applied_departures.is_empty());
    let unknown = definition.replace("classification='patient'", "classification='unknown'");
    assert!(report("F", "", &unknown)?.applied_departures.is_empty());
    for value in ["P", "R", "C", "X", "D"] {
        let mapped = report("F", value, &definition)?;
        assert_eq!(first(&mapped)?.source_status.as_str(), value);
        assert!(mapped.applied_departures.is_empty());
        assert!(
            mapped
                .findings
                .iter()
                .any(|f| f.problem == Problem::DepartureConflict)
        );
    }
    Ok(())
}

#[test]
fn adr0015_hl7_report_omission_never_proves_completion_from_partial_observations() -> TestResult {
    let definition = definition("hl7_absent_obr25_means_final", "absent_obr25_final");
    for status in ["P", "R", "C", "X", "D", "\"\"", "UNKNOWN", ""] {
        let mapped = report("", status, &definition)?;
        assert!(mapped.applied_departures.is_empty(), "{status}");
        assert!(
            mapped
                .findings
                .iter()
                .any(|f| f.problem == Problem::UnqualifiedHl7Report && f.position.field == 25)
        );
        assert!(mapped.requires_parking());
    }
    Ok(())
}

#[test]
fn adr0015_hl7_two_omissions_require_both_separate_guarantees() -> TestResult {
    let mut definition = definition("hl7_absent_obr25_means_final", "absent_obr25_final");
    let fixture = include_str!("../../../../core/config/tests/fixtures/departure.toml");
    let second = fixture
        .lines()
        .find(|line| line.starts_with("deviations = ["))
        .and_then(|line| line.strip_prefix("deviations = ["))
        .and_then(|line| line.strip_suffix(']'))
        .ok_or("declaration fixture")?
        .replace("astm_frame_limit", "hl7_absent_obx11_means_final")
        .replace("compatibility", "absent_obx11_final")
        .replace("protocol=\"astm\"", "protocol=\"hl7\",edition=\"v251\"")
        .replace("astm_results", "oru_r01_patient");
    definition = definition.replace("deviations = [", &format!("deviations = [{second},"));
    let mapped = report("", "", &definition)?;
    assert_eq!(mapped.applied_departures.len(), 2);
    assert!(
        !mapped
            .findings
            .iter()
            .any(|f| f.problem == Problem::UnqualifiedHl7Report
                && matches!(f.position.field, 11 | 25))
    );
    assert_eq!(first(&mapped)?.source_status.as_str(), "");
    assert!(mapped.requires_parking());
    Ok(())
}
