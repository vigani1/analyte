//! The one way the command line asks the engine running on this computer, at its loopback admin address, for what only it knows; the admin key goes only to a listener that first proves it holds that key.
//!
//! Every exchange has a time and size limit, uses no proxy and follows no
//! redirect. Before any request that carries the admin key, the listener must
//! answer a fresh nonce at `GET /proof` with the key's HMAC proof, checked in
//! constant time ([`Token::proves`], ADR 0025); a listener that cannot is
//! "something else", and the key is never sent to it. The key is read from the
//! protected data directory with the engine's own checks and never printed.
use crate::admin::overview::answer::Answer;
use crate::admin::{Token, TokenError, server::token::Nonce};
use crate::installation::config::Program;
use std::net::SocketAddrV4;
use std::time::Duration;

/// How long one exchange with the engine may take in all; the engine answers
/// within its own 1.75 s request budget.
pub(in crate::admin) const TIMEOUT: Duration = Duration::from_secs(5);
/// The most bytes of one answer read: far more than 1024 machines' list.
const LIMIT: usize = 1_048_576;
/// The most bytes of a proof read.
const PROOF_LIMIT: usize = 1024;
/// How long a change through the engine may take: its own setup write budget
/// (10 s) and the time to send and answer (ADR 0028).
const WRITE_TIMEOUT: Duration = Duration::from_secs(20);

/// Why the engine could not be asked, or its answer could not be used.
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum EngineError {
    /// The program file has no `[admin]` table, so the engine serves no admin address.
    #[error(
        "the admin pages are switched off in the settings (no [admin] table), so the running engine cannot be asked"
    )]
    AdminOff,
    /// Nothing listens on the admin address: the engine does not run.
    #[error("nothing answers on the admin address {0}: the engine is not running")]
    NotRunning(SocketAddrV4),
    /// The admin address could not be reached for another reason.
    #[error("the admin address {address} cannot be reached: {source}")]
    Unreachable {
        address: SocketAddrV4,
        #[source]
        source: std::io::Error,
    },
    /// The engine took longer than [`TIMEOUT`].
    #[error("the engine did not answer within {} seconds", TIMEOUT.as_secs())]
    TimedOut,
    /// The admin key file could not be read; its value is never shown.
    #[error("the admin key cannot be read as this user ({0}); run the command as an administrator")]
    Key(#[source] TokenError),
    /// The thread reading the admin key stopped before it finished.
    #[error("the admin key read stopped: {0}")]
    KeyThread(#[source] tokio::task::JoinError),
    /// The HTTP client could not be set up on this system.
    #[error("the request could not be prepared: {0}")]
    Client(#[source] reqwest::Error),
    /// The listener could not prove it holds the admin key, so the key was not sent.
    #[error(
        "something else answers on the admin address: it could not prove it is this Analyte engine, so the admin key was not sent"
    )]
    Unproven,
    /// The exchange broke off.
    #[error("the engine did not answer: {0}")]
    Silent(#[source] reqwest::Error),
    /// The engine answered with a status this request does not expect.
    #[error("the engine answered HTTP {0}")]
    Status(u16),
    /// The answer was longer than its limit.
    #[error("the engine's answer was too large")]
    TooLarge,
    /// The answer is not what this version of Analyte expects.
    #[error("the engine's answer could not be read: {0}")]
    Unreadable(#[source] serde_json::Error),
}

/// Why a change sent to the engine has no answer (ADR 0028).
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum PostError {
    /// The change could not be put into a request; nothing was sent.
    #[error("the request could not be written: {0}")]
    Request(#[source] serde_json::Error),
    /// Nothing reached the engine, so nothing was changed.
    #[error("the engine could not be reached: {0}")]
    NotSent(#[source] reqwest::Error),
    /// It may have reached the engine: whether it changed anything is
    /// unknown, never "not done".
    #[error("the engine did not answer the change within {} seconds", WRITE_TIMEOUT.as_secs())]
    TimedOut,
    /// It may have reached the engine, but the answer broke off or was too
    /// large: whether it changed anything is unknown.
    #[error("{0}")]
    Unanswered(EngineError),
}

impl PostError {
    /// Nothing reached the engine, so nothing was changed.
    pub(in crate::admin) const fn not_sent(&self) -> bool {
        matches!(self, Self::Request(_) | Self::NotSent(_))
    }
}

/// The engine at the admin address, proven to hold the admin key.
pub(in crate::admin) struct Engine {
    client: reqwest::Client,
    address: SocketAddrV4,
    key: Token,
}

impl Engine {
    /// The engine of `program`: its admin page is on, something listens on
    /// the admin address, the key is readable and the listener proved it
    /// holds the key. Nothing secret is sent before that proof.
    /// # Errors
    /// Each step's own reason, as [`EngineError`] names it.
    pub(in crate::admin) async fn proven(program: &Program) -> Result<Self, EngineError> {
        let address = program.admin_address().ok_or(EngineError::AdminOff)?;
        // The engine holds the admin address for as long as it runs.
        match tokio::time::timeout(TIMEOUT, tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                return Err(EngineError::NotRunning(address));
            }
            Ok(Err(source)) => return Err(EngineError::Unreachable { address, source }),
            Err(_) => return Err(EngineError::TimedOut),
        }
        let directory = program.data_directory().to_owned();
        let key = tokio::task::spawn_blocking(move || Token::load(&directory))
            .await
            .map_err(EngineError::KeyThread)?
            .map_err(EngineError::Key)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .map_err(EngineError::Client)?;
        let engine = Self {
            client,
            address,
            key,
        };
        if engine.proves().await {
            Ok(engine)
        } else {
            Err(EngineError::Unproven)
        }
    }

    /// Whether the listener answers a fresh challenge with the proof only the
    /// admin key can give; the request carries no key.
    async fn proves(&self) -> bool {
        #[derive(serde::Deserialize)]
        struct Proof {
            proof: String,
        }
        let Ok(nonce) = Nonce::fresh() else {
            return false;
        };
        let path = format!("/proof?nonce={}", nonce.as_str());
        let Ok((200, body)) = self.exchange(&path, false, PROOF_LIMIT).await else {
            return false;
        };
        serde_json::from_slice::<Proof>(&body)
            .ok()
            .and_then(|answer| unhex(&answer.proof))
            .is_some_and(|answer| self.key.proves(&nonce, &answer))
    }

    /// One GET of `path`, with the admin key when `keyed`, within [`TIMEOUT`]
    /// and `limit` bytes: the status code and the body.
    async fn exchange(
        &self,
        path: &str,
        keyed: bool,
        limit: usize,
    ) -> Result<(u16, Vec<u8>), EngineError> {
        let exchange = async {
            let mut request = self.client.get(format!("http://{}{path}", self.address));
            if keyed {
                request = request.bearer_auth(self.key.form_value());
            }
            let mut response = request.send().await.map_err(|error| {
                if error.is_timeout() {
                    EngineError::TimedOut
                } else {
                    EngineError::Silent(error)
                }
            })?;
            let status = response.status().as_u16();
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(EngineError::Silent)? {
                if chunk.len() > limit.saturating_sub(body.len()) {
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

    /// A GET of `path` with the admin key, to this proven engine only.
    /// # Errors
    /// No answer, a timeout or an answer over the limit.
    pub(in crate::admin) async fn get(&self, path: &str) -> Result<(u16, Vec<u8>), EngineError> {
        self.exchange(path, true, LIMIT).await
    }

    /// A POST of `body` as JSON to `path` with the admin key, to this proven
    /// engine only, within [`WRITE_TIMEOUT`] and `limit` bytes of answer: the
    /// status code and the body. The body may hold keys; it is never logged.
    /// # Errors
    /// [`PostError::not_sent`] tells whether anything may have reached the engine.
    pub(in crate::admin) async fn post(
        &self,
        path: &str,
        body: &impl serde::Serialize,
        limit: usize,
    ) -> Result<(u16, Vec<u8>), PostError> {
        let bytes = serde_json::to_vec(body).map_err(PostError::Request)?;
        let exchange = async {
            let mut response = self
                .client
                .post(format!("http://{}{path}", self.address))
                .bearer_auth(self.key.form_value())
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .timeout(WRITE_TIMEOUT)
                .body(bytes)
                .send()
                .await
                .map_err(|error| {
                    if error.is_connect() {
                        PostError::NotSent(error)
                    } else if error.is_timeout() {
                        PostError::TimedOut
                    } else {
                        PostError::Unanswered(EngineError::Silent(error))
                    }
                })?;
            let status = response.status().as_u16();
            let mut answer = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| PostError::Unanswered(EngineError::Silent(error)))?
            {
                if chunk.len() > limit.saturating_sub(answer.len()) {
                    return Err(PostError::Unanswered(EngineError::TooLarge));
                }
                answer.extend_from_slice(&chunk);
            }
            Ok((status, answer))
        };
        tokio::time::timeout(WRITE_TIMEOUT, exchange)
            .await
            .map_err(|_| PostError::TimedOut)?
    }

    /// The engine's version, from its public `GET /status`.
    /// # Errors
    /// No answer, another status or an unreadable answer.
    pub(in crate::admin) async fn version(&self) -> Result<String, EngineError> {
        #[derive(serde::Deserialize)]
        struct Status {
            version: String,
        }
        let (status, body) = self.exchange("/status", false, PROOF_LIMIT).await?;
        // 503 is the engine saying it cannot read its state: it still answers.
        if status != 200 && status != 503 {
            return Err(EngineError::Status(status));
        }
        serde_json::from_slice::<Status>(&body)
            .map(|status| status.version)
            .map_err(EngineError::Unreadable)
    }

    /// The Overview's verdict as the engine reads it now; a state it could
    /// not read is an unknown verdict saying what, never an empty list.
    /// # Errors
    /// No answer, another status or an unreadable answer.
    pub(in crate::admin) async fn verdict(&self) -> Result<Answer, EngineError> {
        let (status, body) = self.get("/overview/verdict").await?;
        match status {
            200 | 503 => serde_json::from_slice(&body).map_err(EngineError::Unreadable),
            other => Err(EngineError::Status(other)),
        }
    }
}

/// The verdict from the engine of `program`, proven first.
/// # Errors
/// As [`Engine::proven`] and [`Engine::verdict`].
pub(in crate::admin) async fn verdict(program: &Program) -> Result<Answer, EngineError> {
    Engine::proven(program).await?.verdict().await
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
