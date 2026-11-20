//! Answers the app's own pages under `http://analyte.localhost/`: the Engine off screen, its files, and the Start engine request.
//!
//! The pages have no script and the strictest content policy; Start engine
//! is a plain form post, accepted only from the app's own page.
mod assets;
mod page;

pub use page::Off;

/// Where the Engine off screen is.
pub const ENGINE_OFF: &str = "/engine-off";

/// Where the Start engine form posts.
const START: &str = "/start-engine";

/// Headers on every answer: nothing loads from anywhere else, no script
/// runs, no other page may frame it, and pages are never stored.
pub const HEADERS: [(&str, &str); 4] = [
    (
        "content-security-policy",
        "default-src 'self'; script-src 'none'; frame-ancestors 'none'; form-action 'self'; base-uri 'none'",
    ),
    ("x-frame-options", "DENY"),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "no-referrer"),
];

/// One request to the app's pages, as the shell received it.
pub struct Request<'a> {
    /// The method, such as `GET`.
    pub method: &'a str,
    /// The path, such as `/engine-off`.
    pub path: &'a str,
    /// The `Origin` header, which a form post carries.
    pub origin: Option<&'a str>,
}

/// One answer.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The media type.
    pub kind: &'static str,
    /// The bytes.
    pub body: std::borrow::Cow<'static, [u8]>,
    /// Where a redirect goes.
    pub location: Option<&'static str>,
    /// Whether the answer may be kept (the compiled-in files) or not (pages).
    pub store: bool,
}

/// What a request asks the app to do besides answering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Ask the service manager to start the engine.
    StartEngine,
}

impl Answer {
    fn text(status: u16, words: &'static str) -> Self {
        Self {
            status,
            kind: "text/plain; charset=utf-8",
            body: words.as_bytes().into(),
            location: None,
            store: false,
        }
    }
}

/// Answers `request` with the Engine off screen made from `off`, at `now`
/// on this computer's clock; gives the action it asks for, if any.
#[must_use]
pub fn answer(request: &Request<'_>, off: &Off, now: jiff::Timestamp) -> (Answer, Option<Action>) {
    match (request.method, request.path) {
        ("GET", ENGINE_OFF) => {
            let zone = jiff::tz::TimeZone::system();
            let answer = match page::render(off, &zone, now) {
                Ok(html) => Answer {
                    status: 200,
                    kind: "text/html; charset=utf-8",
                    body: html.into_bytes().into(),
                    location: None,
                    store: false,
                },
                Err(error) => {
                    tracing::error!(%error, "the Engine off screen could not be written");
                    Answer::text(500, "The Engine off screen could not be written.")
                }
            };
            (answer, None)
        }
        ("POST", START) => {
            // Only the app's own form may ask; the engine's pages post only to themselves.
            if request.origin != Some(crate::engine::Origin::Local.text().as_str()) {
                return (
                    Answer::text(403, "Only the app's own screen may start the engine."),
                    None,
                );
            }
            let answer = Answer {
                status: 303,
                kind: "text/plain; charset=utf-8",
                body: b"".as_slice().into(),
                location: Some(ENGINE_OFF),
                store: false,
            };
            (answer, Some(Action::StartEngine))
        }
        ("GET", path) => match path.strip_prefix("/static/").and_then(assets::find) {
            Some(asset) => (
                Answer {
                    status: 200,
                    kind: asset.kind,
                    body: asset.bytes.into(),
                    location: None,
                    store: true,
                },
                None,
            ),
            None => (Answer::text(404, "Not found."), None),
        },
        _ => (Answer::text(405, "Not allowed."), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(
        method: &'static str,
        path: &'static str,
        origin: Option<&'static str>,
    ) -> (Answer, Option<Action>) {
        answer(
            &Request {
                method,
                path,
                origin,
            },
            &Off::default(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    // ADR 0019 (Engine off): the screen and its files come from the app; a
    // Start engine post from the app's own page asks for a start and returns
    // to the screen.
    #[test]
    fn the_screen_its_files_and_the_start_post() {
        let (screen, action) = ask("GET", ENGINE_OFF, None);
        assert_eq!((screen.status, action), (200, None));
        assert!(screen.kind.starts_with("text/html"));
        assert!(!screen.store);
        let (css, _) = ask("GET", "/static/analyte.css", None);
        assert_eq!(
            (css.status, css.kind, css.store),
            (200, "text/css; charset=utf-8", true)
        );
        let (font, _) = ask("GET", "/static/fonts/InstrumentSans-Bold.woff2", None);
        assert_eq!(font.kind, "font/woff2");
        let (start, action) = ask("POST", "/start-engine", Some("http://analyte.localhost"));
        assert_eq!((start.status, start.location), (303, Some(ENGINE_OFF)));
        assert_eq!(action, Some(Action::StartEngine));
    }

    // ADR 0019: a start request from anywhere but the app's own page is
    // refused, and nothing else answers.
    #[test]
    fn other_requests_are_refused() {
        for origin in [None, Some("http://127.0.0.1:8080"), Some("null")] {
            let (refused, action) = ask("POST", "/start-engine", origin);
            assert_eq!((refused.status, action), (403, None), "{origin:?}");
        }
        assert_eq!(ask("GET", "/start-engine", None).0.status, 404);
        assert_eq!(ask("GET", "/static/../Cargo.toml", None).0.status, 404);
        assert_eq!(
            ask("GET", "/static/fonts/OFL-IBMPlexMono.txt", None)
                .0
                .status,
            404
        );
        assert_eq!(ask("PUT", ENGINE_OFF, None).0.status, 405);
    }
}
