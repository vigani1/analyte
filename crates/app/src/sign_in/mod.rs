//! Windows sign-in from the app's side (ADR 0021 §6): when to ask the engine's pipe, and where the pages go with its answer.
//!
//! The pages webview carries the user-agent mark [`USER_AGENT`]. The engine
//! answers an unsigned page for that mark with a refresh to
//! `/login/windows?next=<page>`; the window takes that address before it
//! loads (see [`crate::navigation`]), asks the pipe and opens the code. The
//! [`Guard`] lets that happen by itself at most once per 30 s, so a refusal
//! or a session that does not hold never loops.
pub mod line;
#[cfg(windows)]
pub mod pipe;

use crate::engine::Engine;
use line::{Answer, Code, Refusal};
use std::time::{Duration, Instant};

/// The pages webview's user agent. The engine looks only at its end, the
/// mark `AnalyteApp`; the rest names the app for anyone reading a log.
pub const USER_AGENT: &str = concat!("Analyte/", env!("CARGO_PKG_VERSION"), " AnalyteApp");

/// The engine's Windows sign-in address: the app takes it, never loads it
/// without a code.
pub const PATH: &str = "/login/windows";

/// Who started a sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A person: "Sign in with Windows" on the sign-in page, which opens
    /// [`PATH`] with no page to return to.
    Person,
    /// The engine's refresh on an unsigned page, or the window opening; both
    /// name the page to return to.
    Automatic,
}

/// Lets automatic sign-ins through at most once per [`Guard::PAUSE`].
///
/// A person's click always goes through: it happens once per click, so it
/// cannot loop, and it is what someone does right after signing out or being
/// refused, inside the pause. It still restarts the pause, so a session that
/// does not hold after a click cannot loop either. Opening the window is a
/// person's action too and clears the guard ([`Guard::forget`]), so the window
/// signs in at once even right after an earlier try.
#[derive(Debug, Default)]
pub struct Guard {
    last: Option<Instant>,
}

impl Guard {
    /// The least time between two automatic tries.
    pub const PAUSE: Duration = Duration::from_secs(30);

    /// Whether a sign-in started by `trigger` at `now` may ask the pipe; a
    /// try that may starts the pause again.
    pub fn allows(&mut self, trigger: Trigger, now: Instant) -> bool {
        let allowed = match trigger {
            Trigger::Person => true,
            Trigger::Automatic => self
                .last
                .is_none_or(|last| now.saturating_duration_since(last) >= Self::PAUSE),
        };
        if allowed {
            self.last = Some(now);
        }
        allowed
    }

    /// Clears the pause, because a person opened the window.
    pub fn forget(&mut self) {
        self.last = None;
    }
}

/// Where the pages go after a sign-in was asked for.
pub enum Landing {
    /// Redeem the engine's code; the engine then opens the page asked for.
    Code(Code),
    /// The key form with the refusal's fixed sentence.
    Refused(Refusal),
    /// The plain key form: no answer, no pipe, or the guard said no. It never
    /// refreshes, so nothing tries again by itself from there.
    KeyForm,
}

impl From<Answer> for Landing {
    fn from(answer: Answer) -> Self {
        match answer {
            Answer::Code(code) => Self::Code(code),
            Answer::Refused(refusal) => Self::Refused(refusal),
        }
    }
}

impl Landing {
    /// The address on `engine`'s pages: `/login/windows?code=…`,
    /// `/login?windows=<word>` or `/login`.
    #[must_use]
    pub fn address(&self, engine: &Engine) -> String {
        let origin = engine.origin().text();
        match self {
            Self::Code(code) => {
                let code: String =
                    url::form_urlencoded::byte_serialize(code.text().as_bytes()).collect();
                format!("{origin}{PATH}?code={code}")
            }
            Self::Refused(refusal) => format!("{origin}/login?windows={}", refusal.word()),
            Self::KeyForm => format!("{origin}/login"),
        }
    }
}

/// Where the pages go for a sign-in that `trigger` started at `now`: asks
/// the pipe `pipe` within `limits` when the guard allows, and the key form
/// otherwise. Logs say why without the account or the code.
#[cfg(windows)]
pub async fn land(
    guard: &mut Guard,
    pipe: &str,
    next: &str,
    trigger: Trigger,
    now: Instant,
    limits: pipe::Limits,
) -> Landing {
    if !guard.allows(trigger, now) {
        tracing::info!("windows sign-in was tried less than 30 s ago; showing the key form");
        return Landing::KeyForm;
    }
    match pipe::ask(pipe, next, limits).await {
        Ok(answer) => {
            let landing = Landing::from(answer);
            if let Landing::Refused(refusal) = &landing {
                tracing::info!(refusal = refusal.word(), "windows sign-in was refused");
            }
            landing
        }
        Err(error) => {
            tracing::warn!(%error, "windows sign-in gave no answer; showing the key form");
            Landing::KeyForm
        }
    }
}

/// Where the window opens on a running engine: Windows sign-in, landing on
/// the Overview.
#[must_use]
pub fn start(engine: &Engine) -> String {
    format!("{}{PATH}?next=/", engine.origin().text())
}

/// The pipe of the engine whose pages are on `engine`'s port.
#[must_use]
pub fn pipe_name(engine: &Engine) -> String {
    format!(r"\\.\pipe\analyte-sign-in-{}", engine.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::at(8121).expect("a port")
    }

    // ADR 0021 §6: the engine looks for this mark at the end of the user agent.
    #[test]
    fn the_user_agent_ends_in_the_app_mark() {
        assert!(USER_AGENT.ends_with("AnalyteApp"), "{USER_AGENT}");
        assert!(USER_AGENT.is_ascii(), "{USER_AGENT}");
    }

    // ADR 0021 §1: the pipe's name carries the engine's admin port.
    #[test]
    fn the_pipe_is_named_for_the_admin_port() {
        assert_eq!(pipe_name(&engine()), r"\\.\pipe\analyte-sign-in-8121");
        assert_eq!(
            start(&engine()),
            "http://127.0.0.1:8121/login/windows?next=/"
        );
    }

    // ADR 0021 §4 and §6 (#296): a code opens /login/windows?code=, a refusal
    // /login?windows=<word>, anything else the plain key form. The code is
    // standard base64, so its +, / and = are escaped.
    #[test]
    fn each_answer_lands_on_its_address() -> Result<(), line::AnswerError> {
        let code = line::answer(b"{\"code\":\"ab+c/d=\",\"account\":\"x\"}\n")?;
        assert_eq!(
            Landing::from(code).address(&engine()),
            "http://127.0.0.1:8121/login/windows?code=ab%2Bc%2Fd%3D"
        );
        for (refusal, word) in [
            (Refusal::NotAllowed, "not-allowed"),
            (Refusal::BadRequest, "bad-request"),
            (Refusal::Unidentified, "unidentified"),
            (Refusal::Unavailable, "unavailable"),
        ] {
            assert_eq!(
                Landing::Refused(refusal).address(&engine()),
                format!("http://127.0.0.1:8121/login?windows={word}")
            );
        }
        assert_eq!(
            Landing::KeyForm.address(&engine()),
            "http://127.0.0.1:8121/login"
        );
        Ok(())
    }

    /// The pages' address for one sign-in against a fake engine's pipe.
    #[cfg(windows)]
    async fn landed(guard: &mut Guard, pipe: &str, trigger: Trigger, now: Instant) -> String {
        land(guard, pipe, "/parked/12", trigger, now, pipe::fake::QUICK)
            .await
            .address(&engine())
    }

    // The design study's app tests: a code opens the code's address, a
    // refusal the refusal's, and silence the key form after its deadline.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_pages_open_the_code_the_refusal_or_the_key_form() {
        use pipe::fake::{fake, name, serve};
        let at = Instant::now();
        let code = name("land-code");
        let engine_end = tokio::spawn(serve(
            fake(&code),
            b"{\"code\":\"c+d=\",\"account\":\"LAB-PC\\\\maria\"}\n".to_vec(),
        ));
        assert_eq!(
            landed(&mut Guard::default(), &code, Trigger::Automatic, at).await,
            "http://127.0.0.1:8121/login/windows?code=c%2Bd%3D"
        );
        let asked = engine_end.await.expect("the fake engine");
        assert_eq!(asked, b"{\"v\":1,\"next\":\"/parked/12\"}\n");

        let refused = name("land-refused");
        let engine_end = tokio::spawn(serve(
            fake(&refused),
            b"{\"refused\":\"not-allowed\",\"account\":\"LAB-PC\\\\bob\"}\n".to_vec(),
        ));
        assert_eq!(
            landed(&mut Guard::default(), &refused, Trigger::Automatic, at).await,
            "http://127.0.0.1:8121/login?windows=not-allowed"
        );
        engine_end.await.expect("the fake engine");

        let silent = name("land-silent");
        let server = fake(&silent);
        let engine_end = tokio::spawn(async move {
            server.connect().await.expect("a caller");
            tokio::time::sleep(pipe::fake::QUICK.exchange * 3).await;
        });
        let started = Instant::now();
        assert_eq!(
            landed(&mut Guard::default(), &silent, Trigger::Automatic, at).await,
            "http://127.0.0.1:8121/login"
        );
        let waited = started.elapsed();
        assert!(waited >= pipe::fake::QUICK.exchange, "{waited:?}");
        engine_end.await.expect("the fake engine");
        let absent = name("land-absent");
        assert_eq!(
            landed(&mut Guard::default(), &absent, Trigger::Automatic, at).await,
            "http://127.0.0.1:8121/login"
        );
    }

    // ADR 0021 §6 and the design study's app tests: after one automatic try
    // the app does not ask the pipe again by itself within 30 s; it shows the
    // key form without connecting. A person's click asks again.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_app_never_asks_again_by_itself_within_thirty_seconds() {
        use pipe::fake::{fake, name, serve};
        let pipe = name("land-guard");
        let mut guard = Guard::default();
        let at = Instant::now();
        let refusal = b"{\"refused\":\"not-allowed\",\"account\":\"LAB-PC\\\\bob\"}\n".to_vec();
        let engine_end = tokio::spawn(serve(fake(&pipe), refusal.clone()));
        assert_eq!(
            landed(&mut guard, &pipe, Trigger::Automatic, at).await,
            "http://127.0.0.1:8121/login?windows=not-allowed"
        );
        engine_end.await.expect("the fake engine");
        // A fresh instance, never connected: any try would be seen.
        let server = fake(&pipe);
        for later in [1, 15, 29] {
            let again = at + Duration::from_secs(later);
            assert_eq!(
                landed(&mut guard, &pipe, Trigger::Automatic, again).await,
                "http://127.0.0.1:8121/login",
                "after {later} s"
            );
        }
        let engine_end = tokio::spawn(serve(server, refusal));
        assert_eq!(
            landed(
                &mut guard,
                &pipe,
                Trigger::Person,
                at + Duration::from_secs(29)
            )
            .await,
            "http://127.0.0.1:8121/login?windows=not-allowed"
        );
        engine_end.await.expect("the fake engine");
    }

    // ADR 0021 §6: the app tries by itself at most once per 30 s, so a
    // refusal never loops; it may again once the pause is over.
    #[test]
    fn automatic_tries_wait_thirty_seconds() {
        let mut guard = Guard::default();
        let start = Instant::now();
        assert!(guard.allows(Trigger::Automatic, start));
        for later in [0, 1, 10, 29] {
            let at = start + Duration::from_secs(later);
            assert!(!guard.allows(Trigger::Automatic, at), "after {later} s");
        }
        assert!(guard.allows(Trigger::Automatic, start + Duration::from_secs(30)));
        assert!(!guard.allows(Trigger::Automatic, start + Duration::from_secs(31)));
    }

    // The guard decision (PR plan row 4): a person's click always tries and
    // restarts the pause; opening the window clears it.
    #[test]
    fn a_person_may_always_try_and_opening_the_window_clears_the_pause() {
        let mut guard = Guard::default();
        let start = Instant::now();
        assert!(guard.allows(Trigger::Automatic, start));
        let click = start + Duration::from_secs(5);
        assert!(guard.allows(Trigger::Person, click));
        assert!(guard.allows(Trigger::Person, click));
        assert!(!guard.allows(Trigger::Automatic, click + Duration::from_secs(29)));
        assert!(guard.allows(Trigger::Automatic, click + Duration::from_secs(30)));
        guard.forget();
        assert!(guard.allows(Trigger::Automatic, click + Duration::from_secs(31)));
    }
}
