//! Prints `analyte overview`: the running engine's verdict in the page's words, each line with where and how to fix it, and an exit code for scripts.
use crate::admin::Error;
use crate::admin::engine::{self, EngineError};
use crate::admin::overview::answer::{Answer, Severity, Verdict};
use crate::installation::config::Program;
use std::process::ExitCode;

/// Asks the engine and prints its verdict; an engine that does not answer
/// is an unknown state (exit 3), never "All good".
pub(super) async fn run(program: &Program, json: bool) -> Result<ExitCode, Error> {
    let answer = engine::verdict(program).await;
    let verdict = answer
        .as_ref()
        .map_or(Verdict::Unknown, |answer| answer.verdict);
    match (&answer, json) {
        (Ok(answer), true) => super::json(answer)?,
        (Ok(answer), false) => print(answer)?,
        (Err(error), true) => super::json(&Unreached::of(error))?,
        (Err(error), false) => {
            super::say("Analyte cannot say how things are right now")?;
            super::say(&super::chain(error))?;
            super::say(&format!("What to do: {}", advice(error)))?;
        }
    }
    Ok(ExitCode::from(verdict.exit_code()))
}

/// The answer as text: the title, the line under it, then each thing.
fn print(answer: &Answer) -> Result<(), Error> {
    super::say(&answer.title())?;
    super::say(&answer.sub())?;
    for (number, need) in (1u32..).zip(&answer.needs) {
        let severity = match need.severity {
            Severity::Problem => "PROBLEM",
            Severity::Attention => "ATTENTION",
        };
        super::say("")?;
        super::say(&format!("{number:02} {severity}: {}", need.text))?;
        if let Some(detail) = &need.detail {
            super::say(&format!("   {detail}"))?;
        }
        super::say(&format!("   Fix on: {} ({})", need.page, need.href))?;
        if let Some(command) = &need.command {
            super::say(&format!("   Command: {command}"))?;
        }
    }
    Ok(())
}

/// What to do when the engine could not be asked, in plain words.
pub(super) fn advice(error: &EngineError) -> &'static str {
    match error {
        EngineError::AdminOff => {
            "add an [admin] table with a port to the program file and restart the engine (see docs/operations/operate.md)"
        }
        EngineError::Key(_) | EngineError::KeyThread(_) => {
            "run this from an administrator PowerShell; if the key file is missing, run analyte admin token --rotate and restart the engine"
        }
        EngineError::NotRunning(_)
        | EngineError::Unreachable { .. }
        | EngineError::Silent(_)
        | EngineError::TimedOut => {
            "start the engine (Start-Service Analyte, or systemctl start analyte on Linux), wait a few seconds and try again; if it does not start, run analyte doctor and analyte logs"
        }
        EngineError::Unproven => {
            "find the program holding the admin port (Get-NetTCPConnection -LocalPort <port>) and stop it, or restart the engine with Restart-Service Analyte if the admin key was rotated since it started"
        }
        EngineError::Client(_)
        | EngineError::Status(_)
        | EngineError::TooLarge
        | EngineError::Unreadable(_) => {
            "check that this command and the engine are the same Analyte version, then run analyte logs"
        }
    }
}

/// The JSON answer when the engine could not be asked: the same shape as
/// the engine's own unknown answer, so scripts read one format.
#[derive(serde::Serialize)]
struct Unreached {
    verdict: Verdict,
    needs: [(); 0],
    unread: String,
    advice: &'static str,
}

impl Unreached {
    fn of(error: &EngineError) -> Self {
        Self {
            verdict: Verdict::Unknown,
            needs: [],
            unread: super::chain(error),
            advice: advice(error),
        }
    }
}
