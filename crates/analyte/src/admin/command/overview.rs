//! Prints `analyte overview`: the running engine's verdict in the page's words, each line with where and how to fix it, and an exit code for scripts.
use crate::admin::Error;
use crate::admin::engine::{self, EngineError};
use crate::admin::overview::answer::{Answer, Severity, Verdict};
use crate::installation::config::Program;
use analyte_i18n::{message as tr_args, text as tr};
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
            super::say(tr(
                "doctor-overview-analyte-cannot-say-how-things-are-right-now",
            ))?;
            super::say(&super::chain(error))?;
            super::say(&tr_args(
                "doctor-overview-what-to-do",
                [("error", (advice(error)).to_string().into())],
            ))?;
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
            Severity::Problem => tr("doctor-overview-problem"),
            Severity::Attention => tr("doctor-overview-attention"),
        };
        super::say("")?;
        super::say(&format!("{number:02} {severity}: {}", need.text))?;
        if let Some(detail) = &need.detail {
            super::say(&format!("   {detail}"))?;
        }
        super::say(&tr_args(
            "doctor-overview-fix-on",
            [
                ("page", (need.page).to_string().into()),
                ("href", (need.href).to_string().into()),
            ],
        ))?;
        if let Some(command) = &need.command {
            super::say(&tr_args(
                "doctor-overview-command",
                [("command", (command).to_string().into())],
            ))?;
        }
    }
    Ok(())
}

/// What to do when the engine could not be asked, in plain words.
pub(super) fn advice(error: &EngineError) -> &'static str {
    match error {
        EngineError::AdminOff => tr("doctor-overview-add-an-admin-table-with-a-port-to-the"),
        EngineError::Key(_) | EngineError::KeyThread(_) => {
            tr("doctor-overview-run-this-from-an-administrator-powershell-if-the-key")
        }
        EngineError::NotRunning(_)
        | EngineError::Unreachable { .. }
        | EngineError::Silent(_)
        | EngineError::TimedOut => {
            tr("doctor-overview-start-the-engine-start-service-analyte-or-systemctl-start")
        }
        EngineError::Unproven => {
            tr("doctor-overview-find-the-program-holding-the-admin-port-get-nettcpconnection")
        }
        EngineError::Client(_)
        | EngineError::Status(_)
        | EngineError::TooLarge
        | EngineError::Unreadable(_) => {
            tr("doctor-overview-check-that-this-command-and-the-engine-are-the")
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
