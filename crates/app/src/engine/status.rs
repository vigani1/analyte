//! Asks the engine's public `GET /status` how things are, within a time and size limit, and reads its one word.
//!
//! The answer is the Overview's verdict as `good`, `attention` or `problem`,
//! or `unknown` when the engine could not read its own state. Anything else
//! (no connection, a timeout, another program's answer) is not the engine
//! answering, and the app says what it saw instead of guessing a state.
use std::time::Duration;

/// How long one question may take, connection included: the engine reads
/// its state within 1.5 s and answers within 1.75 s.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// The most of an answer the app reads; the real one is about 40 bytes.
const LIMIT: usize = 1024;

/// What the engine said about the lab's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Nothing needs anyone.
    Good,
    /// Something needs a look; nothing stops data now.
    Attention,
    /// Something stops data now.
    Problem,
    /// The engine runs but could not read its own state in time.
    Unknown,
}

impl Health {
    /// The engine's word for this state.
    fn of_word(word: &str) -> Option<Self> {
        match word {
            "good" => Some(Self::Good),
            "attention" => Some(Self::Attention),
            "problem" => Some(Self::Problem),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// The state in plain words, as the tray and notifications say it.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Good => "All good",
            Self::Attention => "Something needs you",
            Self::Problem => "A problem needs you",
            Self::Unknown => "The engine could not read its state",
        }
    }
}

/// What one question to `/status` brought back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The engine answered with its state.
    Answered(Health),
    /// Something answered at the engine's address, but not as the engine
    /// does; what was wrong, in plain words.
    Unrecognised(&'static str),
    /// Nothing answered in time.
    Silent(Silence),
}

/// Why nothing answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    /// Nothing accepts connections at the address: the engine, or its admin
    /// pages, are not running.
    Refused,
    /// The question took longer than [`TIMEOUT`].
    TimedOut,
    /// The connection broke before a whole answer came.
    Broken,
}

impl Silence {
    /// What the app observed, in plain words.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Refused => "Nothing accepts connections at the engine’s address.",
            Self::TimedOut => "The engine did not answer within 3 seconds.",
            Self::Broken => "The connection to the engine broke before it answered.",
        }
    }
}

/// The body `/status` sends; any further field is ignored.
#[derive(serde::Deserialize)]
struct Body {
    state: String,
    version: String,
}

/// Asks one engine's `/status`. It never follows a redirect and never uses
/// a proxy: the engine is on this computer.
pub struct Client {
    http: reqwest::Client,
    url: String,
}

impl Client {
    /// A client for `engine`.
    ///
    /// # Errors
    ///
    /// When the HTTP client cannot be set up on this system.
    pub fn new(engine: &super::Engine) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(TIMEOUT)
                .connect_timeout(TIMEOUT)
                .build()?,
            url: engine.status(),
        })
    }

    /// Asks once, within [`TIMEOUT`] and [`LIMIT`] bytes.
    pub async fn ask(&self) -> Reply {
        match tokio::time::timeout(TIMEOUT, self.exchange()).await {
            Ok(reply) => reply,
            Err(_) => Reply::Silent(Silence::TimedOut),
        }
    }

    async fn exchange(&self) -> Reply {
        let mut response = match self.http.get(&self.url).send().await {
            Ok(response) => response,
            Err(error) => return Reply::Silent(silence(&error)),
        };
        let status = response.status().as_u16();
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len().saturating_add(chunk.len()) > LIMIT {
                        return Reply::Unrecognised("The answer is larger than the engine’s.");
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(error) => return Reply::Silent(silence(&error)),
            }
        }
        read(status, &body)
    }
}

/// The reply a whole answer with `status` and `body` gives.
fn read(status: u16, body: &[u8]) -> Reply {
    let Ok(Body { state, version }) = serde_json::from_slice::<Body>(body) else {
        return Reply::Unrecognised("The answer is not the engine’s status.");
    };
    let Some(health) = Health::of_word(&state) else {
        return Reply::Unrecognised("The answer names a state the app does not know.");
    };
    // The engine answers 200 with its verdict and 503 when it could not read it.
    let expected = if health == Health::Unknown { 503 } else { 200 };
    if status != expected || version.is_empty() {
        return Reply::Unrecognised("The answer is not the engine’s status.");
    }
    Reply::Answered(health)
}

fn silence(error: &reqwest::Error) -> Silence {
    if error.is_timeout() {
        Silence::TimedOut
    } else if error.is_connect() {
        Silence::Refused
    } else {
        Silence::Broken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0019: the four words the engine's `/status` answers with, each with
    // the status code it comes with.
    #[test]
    fn the_engine_words_read_as_states() {
        for (status, word, health) in [
            (200, "good", Health::Good),
            (200, "attention", Health::Attention),
            (200, "problem", Health::Problem),
            (503, "unknown", Health::Unknown),
        ] {
            let body = format!("{{\"state\":\"{word}\",\"version\":\"0.1.0\"}}");
            assert_eq!(read(status, body.as_bytes()), Reply::Answered(health));
        }
    }

    // AGENTS.md (never guess): an answer that is not the engine's is never
    // read as a state, so another program on the port is not "All good".
    #[test]
    fn other_answers_are_not_a_state() {
        for (status, body) in [
            (200, "<html>sign in</html>"),
            (200, "{\"state\":\"fine\",\"version\":\"0.1.0\"}"),
            (200, "{\"state\":\"good\"}"),
            (200, "{\"state\":\"good\",\"version\":\"\"}"),
            (503, "{\"state\":\"good\",\"version\":\"0.1.0\"}"),
            (200, "{\"state\":\"unknown\",\"version\":\"0.1.0\"}"),
            (401, "{\"state\":\"good\",\"version\":\"0.1.0\"}"),
        ] {
            assert!(
                matches!(read(status, body.as_bytes()), Reply::Unrecognised(_)),
                "{status} {body}"
            );
        }
    }
}
