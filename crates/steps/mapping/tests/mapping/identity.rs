//! Tests source-selected run identity and vendor status meaning.
use super::*;

const RUN: &str = "run = [{group='observation',record='R',field=13,component=1,repetition=1}]";
const EVIDENCE: &str = "evidence = {manual='Synthetic example',section='Run identity'}";

fn recipe(codes: &str) -> String {
    ASTM_CONFIG.replace("run = []", RUN).replace(
        "correction_status_codes = [\"C\"]",
        &format!("{EVIDENCE}\n{codes}"),
    )
}

// LIS02-A2 sections 9.9 and 9.13; cobas e 411 HIM v2.3 section 6 Table 18 for C.
// The synthetic upload combines a completion selector with the rerun status;
// Roche's cobas dialect does not send this completion field.
#[test]
fn rerun_code_preserves_source_status_and_distinguishes_runs() -> TestResult {
    let definition = recipe("correction_status_codes = []\nrerun_status_codes = ['C']");
    let map = |status, completion| {
        astm_with(
            &format!("R|1|^^^SYN|1|||||{status}||||{completion}\r"),
            "P",
            &definition,
        )
    };
    let initial = map("F", "20261115123000")?;
    let rerun = map("C", "20261115123100")?;
    let resend = map("C", "20261115123100")?;
    let missing = map("C", "")?;
    let result = |output: Output| output.results.into_iter().next().ok_or("missing result");
    assert_eq!(first(&rerun)?.source_status.as_str(), "C");
    assert_eq!(first(&rerun)?.status, ResultStatus::Unknown);
    assert_eq!(
        rerun
            .results
            .first()
            .ok_or("missing rerun")?
            .corrections
            .as_slice(),
        &[false]
    );
    let rerun = result(rerun)?.content.identity;
    assert!(matches!(rerun, IdentityStatus::Resolved(_)));
    assert_eq!(rerun, result(resend)?.content.identity);
    assert_ne!(rerun, result(initial)?.content.identity);
    assert!(matches!(
        result(missing)?.content.identity,
        IdentityStatus::Unresolved(_)
    ));
    let generic = astm("R|1|^^^SYN|1|||||C\r", "P")?;
    assert_eq!(first(&generic)?.status, ResultStatus::Corrected);
    Ok(())
}

// ADR 0035: an incomplete or contradictory recipe cannot be selected.
#[test]
fn identity_recipe_refuses_missing_evidence_empty_run_and_overlapping_codes() {
    let overlap = recipe("correction_status_codes = ['C']\nrerun_status_codes = ['C']");
    assert!(
        matches!(config(&overlap), Err(analyte_config::Error::Invalid { key, .. })
        if key == "mappings.identity.rerun_status_codes")
    );
    let no_citation = overlap.replace(EVIDENCE, "");
    assert!(
        matches!(config(&no_citation), Err(analyte_config::Error::Invalid { key, .. })
        if key == "mappings.identity.rerun_status_codes")
    );
    let empty_run = recipe("correction_status_codes = []").replace(RUN, "run = []");
    assert!(
        matches!(config(&empty_run), Err(analyte_config::Error::Invalid { key, .. })
        if key == "mappings.identity.run")
    );
    assert!(config(&empty_run.replace(EVIDENCE, "unknown_recipe_key = 'invalid'")).is_err());
}

// LIS02-A2 section 9.13 and ADR 0035: a run field cannot substitute for a sample ID.
#[test]
fn identity_recipe_holds_missing_sample() -> TestResult {
    let definition = recipe("correction_status_codes = ['C']").replace(
        "source = \"barcode\"\n[mappings.sample.field]\ngroup = \"order\"\nrecord = \"O\"\nfield = 3",
        "source = \"barcode\"\n[mappings.sample.field]\ngroup = \"order\"\nrecord = \"O\"\nfield = 4",
    );
    let output = astm_with("R|1|^^^SYN|1|||||F||||20261115123000\r", "P", &definition)?;
    let result = output.results.first().ok_or("missing result")?;
    assert!(result.content.sample.is_none());
    assert!(matches!(
        result.content.identity,
        IdentityStatus::Unresolved(_)
    ));
    Ok(())
}

// cobas e 411 HIM v2.3 section 6 Table 18: the status describes the run.
// This synthetic recipe selects the status as a discriminator in addition to
// the completion field. A second rerun still needs its own source evidence.
#[test]
fn selected_rerun_marker_can_distinguish_the_key() -> TestResult {
    let definition = recipe("correction_status_codes = []\nrerun_status_codes = ['C']")
        .replace(RUN, "run = [{group='observation',record='R',field=13,component=1,repetition=1}, {group='observation',record='R',field=9,component=1,repetition=1}]");
    let map = |status| {
        astm_with(
            &format!("R|1|^^^SYN|1|||||{status}||||20261115123000\r"),
            "P",
            &definition,
        )
    };
    let initial = map("F")?;
    let rerun = map("C")?;
    let initial = initial.results.first().ok_or("missing initial")?;
    let rerun = rerun.results.first().ok_or("missing rerun")?;
    assert!(matches!(
        initial.content.identity,
        IdentityStatus::Resolved(_)
    ));
    assert!(matches!(
        rerun.content.identity,
        IdentityStatus::Resolved(_)
    ));
    assert_ne!(initial.content.identity, rerun.content.identity);
    Ok(())
}

// LIS02-A2 sections 9.9 and 9.13: correction keeps the completion key.
#[test]
fn correction_code_keeps_the_measurement_key() -> TestResult {
    let definition = recipe("correction_status_codes = ['C']");
    let initial = astm_with("R|1|^^^SYN|1|||||F||||20261115123000\r", "P", &definition)?;
    let correction = astm_with("R|1|^^^SYN|2|||||C||||20261115123000\r", "P", &definition)?;
    let initial = initial.results.first().ok_or("missing initial")?;
    let correction = correction.results.first().ok_or("missing correction")?;
    assert!(matches!(
        initial.content.identity,
        IdentityStatus::Resolved(_)
    ));
    assert_eq!(initial.content.identity, correction.content.identity);
    assert_eq!(correction.corrections.as_slice(), &[true]);
    Ok(())
}
