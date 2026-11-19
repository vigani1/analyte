//! Runs the published contract v1 vectors against a lab app's real endpoints and reports each case.
#![forbid(unsafe_code)]
mod client;
mod deliveries;
mod orders;
pub mod settings;
mod vectors;

use serde_json::Value;
pub use settings::Settings;
use std::{fmt::Write as _, io::Read as _, path::Path};

/// The seed file the docs ask app developers to load before a seeded run.
pub const SEED_FILE: &str = "docs/contract/vectors/seed-orders.json";

/// One case's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The app did what the contract requires.
    Pass,
    /// The app broke the rule; the text says how, in one line.
    Fail(String),
    /// Not checked in this run; the text says why and what to do.
    Skip(String),
}

/// A named case, the contract section it checks and its outcome.
#[derive(Debug, Clone)]
pub struct Case {
    /// Stable name: the vector file stem, then the vector's own name.
    pub name: String,
    /// The `docs/contract/v1.md` section the case checks.
    pub rule: &'static str,
    /// What happened.
    pub outcome: Outcome,
}

/// Every case of one run, in the order they ran.
#[derive(Debug, Default)]
pub struct Report {
    /// Cases in run order.
    pub cases: Vec<Case>,
}

impl Report {
    fn add(&mut self, name: impl Into<String>, rule: &'static str, outcome: Outcome) {
        self.cases.push(Case {
            name: name.into(),
            rule,
            outcome,
        });
    }

    /// True when no case failed; skipped cases do not fail a run.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self
            .cases
            .iter()
            .any(|case| matches!(case.outcome, Outcome::Fail(_)))
    }

    /// Names of the failed cases, in run order.
    #[must_use]
    pub fn failed(&self) -> Vec<&str> {
        self.cases
            .iter()
            .filter(|case| matches!(case.outcome, Outcome::Fail(_)))
            .map(|case| case.name.as_str())
            .collect()
    }

    /// The table printed by the command: one row per case, a reason line under FAIL and SKIP.
    #[must_use]
    pub fn render(&self) -> String {
        let width = self
            .cases
            .iter()
            .map(|case| case.name.len())
            .max()
            .unwrap_or(4)
            .max(4);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<6}  {:<width$}  RULE (docs/contract/v1.md)",
            "RESULT", "CASE"
        );
        let (mut passed, mut failed, mut skipped) = (0_usize, 0_usize, 0_usize);
        for case in &self.cases {
            let (label, reason) = match &case.outcome {
                Outcome::Pass => {
                    passed = passed.saturating_add(1);
                    ("PASS", None)
                }
                Outcome::Fail(reason) => {
                    failed = failed.saturating_add(1);
                    ("FAIL", Some(reason))
                }
                Outcome::Skip(reason) => {
                    skipped = skipped.saturating_add(1);
                    ("SKIP", Some(reason))
                }
            };
            let _ = writeln!(out, "{label:<6}  {:<width$}  {}", case.name, case.rule);
            if let Some(reason) = reason {
                let _ = writeln!(out, "{:<6}  {}", "", client::one_line(reason));
            }
        }
        let _ = writeln!(out, "{passed} passed, {failed} failed, {skipped} skipped");
        out
    }
}

/// The run could not start.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The HTTP client could not be built (for example, TLS roots unavailable).
    #[error("HTTP client could not start: {0}")]
    Client(#[source] reqwest::Error),
    /// A published vector embedded in the program is not what the checker expects.
    #[error("published vector {0} is malformed")]
    Vector(&'static str),
    /// The seed file is not a JSON array of at most 100 valid contract orders.
    #[error("seed file must be a JSON array of at most 100 valid contract v1 orders")]
    Seed,
    /// The seed file could not be opened or read.
    #[error("seed file could not be read: {0}")]
    SeedFile(#[source] std::io::Error),
}

/// Reads a seed file of at most one document's size and parses it like [`parse_seed`].
///
/// # Errors
/// Returns `RunError::SeedFile` when the file cannot be read and `RunError::Seed` when
/// it is too large or not at most 100 valid contract orders.
pub fn read_seed(path: &Path) -> Result<Vec<analyte_contract::Order>, RunError> {
    let limit = u64::try_from(analyte_contract::MAX_DOCUMENT_BYTES).unwrap_or(u64::MAX);
    parse_seed(&read_bounded(path, limit).map_err(RunError::SeedFile)?)
}

/// Parses a seed file: the orders the app developer loaded into the app.
///
/// # Errors
/// Returns `RunError::Seed` for anything but at most 100 valid contract orders.
pub fn parse_seed(bytes: &[u8]) -> Result<Vec<analyte_contract::Order>, RunError> {
    let limit = limit("max_seed_orders")?;
    analyte_contract::decode::<Vec<analyte_contract::Order>>(bytes)
        .ok()
        .filter(|orders| orders.len() <= limit)
        .ok_or(RunError::Seed)
}

/// Runs every applicable case against the app. Cases that need seeded orders are
/// skipped, with instructions, when `seed` is `None`.
///
/// Each request is bounded by the settings' deadline; cancelling the future may leave
/// a delivery stored in the app under a fresh synthetic delivery ID.
///
/// # Errors
/// Returns `RunError` only when the run cannot start; app failures are cases.
pub async fn run(
    settings: Settings,
    seed: Option<&[analyte_contract::Order]>,
) -> Result<Report, RunError> {
    let app = client::App::new(settings).map_err(RunError::Client)?;
    let mut report = Report::default();
    deliveries::check(&app, &mut report).await?;
    orders::check(&app, seed, &mut report).await?;
    deliveries::check_retry_after(&app, &mut report);
    Ok(report)
}

/// Reads at most `limit` bytes plus one, so the caller can tell a file over the limit.
fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Reads one number from `limits.json`.
fn limit(name: &'static str) -> Result<usize, RunError> {
    serde_json::from_slice::<Value>(vectors::LIMITS)
        .ok()
        .and_then(|limits| limits.get(name)?.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(RunError::Vector("limits.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract v1 "Test vectors": the command prints one row per case with a reason line
    #[test]
    fn table_has_reason_under_fail_and_skip() {
        let mut report = Report::default();
        report.add("a.pass", "POST /results", Outcome::Pass);
        report.add(
            "b.fail",
            "GET /orders",
            Outcome::Fail("got\nHTTP 500".into()),
        );
        report.add("c.skip", "GET /orders", Outcome::Skip("seed first".into()));
        let table = report.render();
        assert!(table.contains("PASS    a.pass"));
        assert!(table.contains("FAIL    b.fail"));
        assert!(table.contains("        got\\nHTTP 500"));
        assert!(table.contains("1 passed, 1 failed, 1 skipped"));
        assert!(!report.passed());
        assert_eq!(report.failed(), ["b.fail"]);
    }

    // Contract v1 "GET /orders": seed files hold at most 100 orders
    #[test]
    fn published_seed_parses() -> Result<(), RunError> {
        let seed = parse_seed(include_bytes!(
            "../../../../docs/contract/vectors/seed-orders.json"
        ))?;
        assert!(seed.len() >= 3);
        assert!(parse_seed(b"{}").is_err());
        Ok(())
    }
}
