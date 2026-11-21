//! Asks the running engine how its machines stand, through its own admin address and the admin key, for `machines list`.
//!
//! Link states live only in the running engine, so the command reads them the
//! way the Machines page does: `GET /machines.json` on the configured loopback
//! admin address, with the admin key read from the protected data directory.
//! The key is sent in a header to that address only, and only after the
//! listener proved it holds the key ([`proven`]); it is never printed or logged.
//! Anything else than a good answer within [`WAIT`] means the live state is
//! unknown, said with the reason; a state is never guessed.
use crate::admin::{Token, server::token::Nonce};
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

/// Why the key was not sent: the listener did not prove it is the engine.
const UNPROVEN: &str = "something else answers on the admin address: it could not prove it is this Analyte engine, so the admin key was not sent";

/// The largest proof answer read.
const PROOF_LIMIT: usize = 1024;

#[derive(serde::Deserialize)]
struct Proof {
    proof: String,
}

/// Whether the listener at `address` answers a fresh challenge with the
/// proof only the admin key can give; nothing secret is sent to ask.
async fn proven(client: &reqwest::Client, address: std::net::SocketAddrV4, token: &Token) -> bool {
    let Ok(nonce) = Nonce::fresh() else {
        return false;
    };
    let request = client.get(format!("http://{address}/proof?nonce={}", nonce.as_str()));
    let Ok(body) = fetch(request, PROOF_LIMIT).await else {
        return false;
    };
    serde_json::from_slice::<Proof>(&body)
        .ok()
        .and_then(|answer| unhex(&answer.proof))
        .is_some_and(|answer| token.proves(&nonce, &answer))
}

/// Bytes from lowercase or uppercase hex; `None` for anything else.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at.saturating_add(2))?, 16).ok())
        .collect()
}

/// Sends `request` and reads a good answer of at most `limit` bytes.
async fn fetch(request: reqwest::RequestBuilder, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("the engine did not answer: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("the engine answered {}", response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("the answer was cut off: {error}"))?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err("the engine's answer was too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
    // The key goes only to a listener that proves it holds it (ADR 0025).
    if !proven(&client, address, &token).await {
        return Live::Unknown(UNPROVEN.into());
    }
    let request = client
        .get(format!("http://{address}/machines.json"))
        .bearer_auth(token.form_value());
    let body = match fetch(request, ANSWER_LIMIT).await {
        Ok(body) => body,
        Err(why) => return Live::Unknown(why),
    };
    match serde_json::from_slice::<Answer>(&body) {
        Ok(answer) => Live::Running {
            settings_read: answer.settings_read,
            machines: answer.machines,
        },
        Err(error) => Live::Unknown(format!("the answer could not be read: {error}")),
    }
}
