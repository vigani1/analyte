//! Asks the running engine how its machines stand, through its own admin address and the admin key, for `machines list`.
//!
//! Link states live only in the running engine, so the command reads them the
//! way the Machines page does: `GET /machines.json` on the configured loopback
//! admin address, with the admin key read from the protected data directory.
//! The key is sent in a header to that address only, and only after the
//! listener proved it holds the key ([`crate::admin::engine`]); it is never
//! printed or logged.
//! Anything else than a good answer within [`WAIT`] means the live state is
//! unknown, said with the reason; a state is never guessed.
use crate::admin::engine::{Engine, EngineError};
use crate::installation::config::Program;

/// What the engine reported for one machine, in the Machines page's words.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct Row {
    pub(super) id: String,
    /// The engine runs it now, so its port or address is in use.
    pub(super) running: bool,
    pub(super) status: String,
    pub(super) exact: String,
    pub(super) note: Option<String>,
    pub(super) last_data: Option<Time>,
    pub(super) since: Option<Time>,
    pub(super) model: Option<String>,
    pub(super) lab_app: Option<String>,
}

/// A time as the page shows it.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct Time {
    pub(super) datetime: String,
    pub(super) shown: String,
    pub(super) exact: String,
}

#[derive(serde::Deserialize)]
struct Answer {
    settings_read: bool,
    machines: Vec<Row>,
}

/// The engine's answer, or why there is none.
pub(super) enum Live {
    Running {
        /// The engine could read the settings file; if not, recently changed machines may be missing.
        settings_read: bool,
        machines: Vec<Row>,
    },
    /// Nothing answers on the admin address.
    NotRunning,
    /// It cannot be asked or did not answer well; the text says why.
    Unknown(String),
}

/// Asks the engine of `program` for its Machines list.
pub(super) async fn read(program: &Program) -> Live {
    let engine = match Engine::proven(program).await {
        Ok(engine) => engine,
        Err(EngineError::NotRunning(_)) => return Live::NotRunning,
        Err(error) => return Live::Unknown(error.to_string()),
    };
    let body = match engine.get("/machines.json").await {
        Ok((200, body)) => body,
        Ok((status, _)) => return Live::Unknown(EngineError::Status(status).to_string()),
        Err(error) => return Live::Unknown(error.to_string()),
    };
    match serde_json::from_slice::<Answer>(&body) {
        Ok(answer) => Live::Running {
            settings_read: answer.settings_read,
            machines: answer.machines,
        },
        Err(error) => Live::Unknown(EngineError::Unreadable(error).to_string()),
    }
}
