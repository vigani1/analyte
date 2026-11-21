//! Asks the running engine, through its own admin address and the admin key: how its machines stand, for `machines list`, and to store a lab app's keys, for `lab-apps add` and `lab-apps keys` (ADR 0028).
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

/// How long a write through the engine may take: the engine's own setup
/// write budget (10 s) and the time to send and answer.
const WRITE_WAIT: Duration = Duration::from_secs(20);

#[derive(serde::Deserialize)]
struct Proof {
    proof: String,
}

/// The engine on the admin address, proven to hold the admin key; only a
/// request to it carries that key.
pub(super) struct Engine {
    client: reqwest::Client,
    address: std::net::SocketAddrV4,
    token: Token,
}

/// Whether the running engine can be asked.
pub(super) enum Reach {
    /// It answers and proved it holds the admin key.
    Proven(Engine),
    /// Nothing answers on the admin address.
    NotRunning,
    /// The settings switch the admin pages off, so it cannot be asked.
    NoAdmin,
    /// It cannot be asked, or something else answers; the text says why.
    Unknown(String),
}

/// Why a write through the engine has no answer.
pub(super) enum Unanswered {
    /// Nothing reached the engine, so nothing was changed.
    NotSent(String),
    /// It may have reached the engine: whether it was stored is unknown.
    Unknown(String),
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
    let response = request
        .send()
        .await
        .map_err(|error| format!("the engine did not answer: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("the engine answered {}", response.status()));
    }
    read_body(response, limit).await
}

/// The body of `response`, at most `limit` bytes.
async fn read_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
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

/// Finds the engine of `program` and has it prove itself before anything
/// secret is sent (ADR 0025).
pub(super) async fn reach(program: &Program) -> Reach {
    let Some(address) = program.admin_address() else {
        return Reach::NoAdmin;
    };
    // Nothing listening there means the engine does not run; it holds the
    // admin address for as long as it runs.
    match tokio::time::timeout(WAIT, tokio::net::TcpStream::connect(address)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            return Reach::NotRunning;
        }
        Ok(Err(error)) => {
            return Reach::Unknown(format!("the admin address cannot be reached: {error}"));
        }
        Err(_) => return Reach::Unknown("the admin address did not answer in time".into()),
    }
    let token = match Token::load(program.data_directory()) {
        Ok(token) => token,
        Err(error) => {
            return Reach::Unknown(format!(
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
        Err(error) => return Reach::Unknown(format!("the request could not be prepared: {error}")),
    };
    // The key goes only to a listener that proves it holds it (ADR 0025).
    if !proven(&client, address, &token).await {
        return Reach::Unknown(UNPROVEN.into());
    }
    Reach::Proven(Engine {
        client,
        address,
        token,
    })
}

impl Engine {
    /// `GET path` with the admin key; a good answer of at most `limit` bytes.
    async fn get(&self, path: &str, limit: usize) -> Result<Vec<u8>, String> {
        let request = self
            .client
            .get(format!("http://{}{path}", self.address))
            .bearer_auth(self.token.form_value());
        fetch(request, limit).await
    }

    /// `POST path` with `body` as JSON and the admin key, within
    /// [`WRITE_WAIT`]; the answer's status and at most `limit` bytes of it.
    /// The body is never logged. When the request may have reached the engine
    /// but no answer came, the write's outcome is unknown, never "not done".
    pub(super) async fn post(
        &self,
        path: &str,
        body: &impl serde::Serialize,
        limit: usize,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), Unanswered> {
        let bytes = serde_json::to_vec(body).map_err(|error| {
            Unanswered::NotSent(format!("the request could not be written: {error}"))
        })?;
        let response = self
            .client
            .post(format!("http://{}{path}", self.address))
            .bearer_auth(self.token.form_value())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(WRITE_WAIT)
            .body(bytes)
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() {
                    Unanswered::NotSent(format!("the engine could not be reached: {error}"))
                } else {
                    Unanswered::Unknown(format!("the engine did not answer: {error}"))
                }
            })?;
        let status = response.status();
        let answer = read_body(response, limit)
            .await
            .map_err(Unanswered::Unknown)?;
        Ok((status, answer))
    }
}

/// Asks the engine of `program` for its Machines list.
pub(super) async fn read(program: &Program) -> Live {
    let engine = match reach(program).await {
        Reach::Proven(engine) => engine,
        Reach::NotRunning => return Live::NotRunning,
        Reach::NoAdmin => {
            return Live::Unknown("the admin pages are switched off in the settings".into());
        }
        Reach::Unknown(why) => return Live::Unknown(why),
    };
    let body = match engine.get("/machines.json", ANSWER_LIMIT).await {
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
