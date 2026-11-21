//! Asks the engine running on this computer, at its local admin address, for what only it knows: whether it answers, and the Overview's verdict now.
//!
//! The engine alone holds each machine's live link and whether its service
//! still runs, so the command line asks it instead of guessing from the
//! store. Every request has a time and size limit, never uses a proxy and
//! never follows a redirect. The verdict needs the admin key, read from the
//! protected data directory the way the engine reads it; the key is sent only
//! to the loopback admin address and never printed.
use crate::admin::Token;
use crate::admin::overview::answer::Answer;
use crate::installation::config::Program;
use std::time::Duration;

/// How long one request to the engine may take in all; the engine answers
/// within its own 1.75 s request budget.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(5);
/// The most bytes of an answer read; a verdict is a few kilobytes.
const LIMIT: usize = 256 * 1024;

/// Why the engine could not be asked or its answer could not be used.
#[derive(Debug, thiserror::Error)]
pub(crate) enum EngineError {
    /// The program file has no `[admin]` table, so the engine serves no admin address.
    #[error(
        "the admin page is off (the program file has no [admin] table), so the running engine cannot be asked"
    )]
    AdminOff,
    /// The admin key file could not be read; its value is never shown.
    #[error("the admin key could not be read from the data directory: {0}")]
    Key(#[source] crate::admin::TokenError),
    /// The thread reading the admin key stopped before it finished.
    #[error("the admin key read stopped: {0}")]
    KeyThread(#[source] tokio::task::JoinError),
    /// The HTTP client could not be set up on this system.
    #[error("the HTTP client could not be set up: {0}")]
    Client(#[source] reqwest::Error),
    /// Nothing answered at the admin address: the engine is stopped, starting,
    /// or another program holds the port.
    #[error("the engine is not answering at {origin}: {source}")]
    Silent {
        origin: String,
        #[source]
        source: reqwest::Error,
    },
    /// The engine took longer than [`TIMEOUT`].
    #[error("the engine did not answer within {} seconds", TIMEOUT.as_secs())]
    TimedOut,
    /// The engine refused the admin key: it was rotated since the engine started.
    #[error(
        "the engine refused the admin key (HTTP {0}); if the key was rotated, restart the engine with Restart-Service Analyte"
    )]
    KeyRefused(u16),
    /// The engine answered with another status.
    #[error("the engine answered HTTP {0}")]
    Status(u16),
    /// The answer was longer than [`LIMIT`].
    #[error("the engine's answer is longer than {LIMIT} bytes")]
    TooLarge,
    /// The answer is not what this version of Analyte expects.
    #[error("the engine's answer could not be read: {0}")]
    Unreadable(#[source] serde_json::Error),
}

/// Where the engine serves its admin pages, such as `http://127.0.0.1:8080`.
fn origin(program: &Program) -> Result<String, EngineError> {
    program
        .admin_address()
        .map(|address| format!("http://{address}"))
        .ok_or(EngineError::AdminOff)
}

fn client() -> Result<reqwest::Client, EngineError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(TIMEOUT)
        .connect_timeout(TIMEOUT)
        .build()
        .map_err(EngineError::Client)
}

/// One GET of `path`, with the admin key when `key` is given, within
/// [`TIMEOUT`] and [`LIMIT`]: the status code and the body.
async fn get(
    program: &Program,
    path: &str,
    key: Option<&Token>,
) -> Result<(u16, Vec<u8>), EngineError> {
    let origin = origin(program)?;
    let client = client()?;
    let exchange = async {
        let mut request = client.get(format!("{origin}{path}"));
        if let Some(key) = key {
            request = request.bearer_auth(key.form_value());
        }
        let mut response = request.send().await.map_err(|source| {
            if source.is_timeout() {
                EngineError::TimedOut
            } else {
                EngineError::Silent {
                    origin: origin.clone(),
                    source,
                }
            }
        })?;
        let status = response.status().as_u16();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|source| EngineError::Silent {
                origin: origin.clone(),
                source,
            })?
        {
            if chunk.len() > LIMIT.saturating_sub(body.len()) {
                return Err(EngineError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    };
    tokio::time::timeout(TIMEOUT, exchange)
        .await
        .map_err(|_| EngineError::TimedOut)?
}

/// The engine's version when it answers its public `GET /status`, which
/// needs no key and says nothing but the overall state.
/// # Errors
/// Reports an admin page that is off, no answer, a timeout or another status.
pub(crate) async fn answering(program: &Program) -> Result<String, EngineError> {
    #[derive(serde::Deserialize)]
    struct Status {
        version: String,
    }
    let (status, body) = get(program, "/status", None).await?;
    // 503 is the engine saying it cannot read its state: it still answers.
    if status != 200 && status != 503 {
        return Err(EngineError::Status(status));
    }
    serde_json::from_slice::<Status>(&body)
        .map(|status| status.version)
        .map_err(EngineError::Unreadable)
}

/// The Overview's verdict, as the engine reads it now. An engine that could
/// not read its state answers with an unknown verdict and what it could not
/// read, never an empty list.
/// # Errors
/// Reports an admin page that is off, an unreadable key, no answer, a
/// refused key, another status or an unreadable answer.
pub(crate) async fn verdict(program: &Program) -> Result<Answer, EngineError> {
    // An admin page that is off says so before any key is read.
    origin(program)?;
    let directory = program.data_directory().to_owned();
    let key = tokio::task::spawn_blocking(move || Token::load(&directory))
        .await
        .map_err(EngineError::KeyThread)?
        .map_err(EngineError::Key)?;
    let (status, body) = get(program, "/overview/verdict", Some(&key)).await?;
    match status {
        200 | 503 => serde_json::from_slice(&body).map_err(EngineError::Unreadable),
        // Without an accepted key the engine sends people to its sign-in page.
        401 | 303 => Err(EngineError::KeyRefused(status)),
        other => Err(EngineError::Status(other)),
    }
}
