//! Asks the running engine how its machines stand, through its own admin address and the admin key, for `machines list`.
//!
//! Link states live only in the running engine, so the command reads them the
//! way the Machines page does: `GET /machines.json` on the configured loopback
//! admin address, with the admin key read from the protected data directory.
//! The key is sent in a header to that address only, never printed or logged.
//! Anything else than a good answer within [`WAIT`] means the live state is
//! unknown, said with the reason; a state is never guessed.
use crate::admin::Token;
use crate::installation::config::Program;
use std::time::Duration;

/// How long the engine has to answer.
const WAIT: Duration = Duration::from_secs(3);
/// The largest answer read: far more than the Machines list of 1024 machines.
const ANSWER_LIMIT: usize = 1_048_576;

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
    let Some(address) = program.admin_address() else {
        return Live::Unknown("the admin pages are switched off in the settings".into());
    };
    // Nothing listening there means the engine does not run; it holds the
    // admin address for as long as it runs.
    match tokio::time::timeout(WAIT, tokio::net::TcpStream::connect(address)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            return Live::NotRunning;
        }
        Ok(Err(error)) => {
            return Live::Unknown(format!("the admin address cannot be reached: {error}"));
        }
        Err(_) => return Live::Unknown("the admin address did not answer in time".into()),
    }
    let token = match Token::load(program.data_directory()) {
        Ok(token) => token,
        Err(error) => {
            return Live::Unknown(format!(
                "the admin key cannot be read as this user ({error}); run the command as an administrator"
            ));
        }
    };
    let client = match reqwest::Client::builder()
        .timeout(WAIT)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
    {
        Ok(client) => client,
        Err(error) => return Live::Unknown(format!("the request could not be prepared: {error}")),
    };
    let response = client
        .get(format!("http://{address}/machines.json"))
        .bearer_auth(token.form_value())
        .send()
        .await;
    let mut response = match response {
        Ok(response) => response,
        Err(error) => return Live::Unknown(format!("the engine did not answer: {error}")),
    };
    if !response.status().is_success() {
        return Live::Unknown(format!("the engine answered {}", response.status()));
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len().saturating_add(chunk.len()) > ANSWER_LIMIT {
                    return Live::Unknown("the engine's answer was too large".into());
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(error) => return Live::Unknown(format!("the answer was cut off: {error}")),
        }
    }
    match serde_json::from_slice::<Answer>(&body) {
        Ok(answer) => Live::Running {
            settings_read: answer.settings_read,
            machines: answer.machines,
        },
        Err(error) => Live::Unknown(format!("the answer could not be read: {error}")),
    }
}
