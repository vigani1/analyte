//! Answers the app's own pages under `http://analyte.localhost/`: the Engine off screen, its files, the Start engine request, and the title bar.
//!
//! The Engine off screen has no script and the strictest content policy;
//! Start engine is a plain form post, accepted only from the app's own page.
//! The title bar runs its one script file and may reach Tauri's IPC; it is
//! answered only to the bar's own webview, the one its permissions name.
mod assets;
mod page;

pub use page::Off;

use crate::title_bar;

/// Where the Engine off screen is.
pub const ENGINE_OFF: &str = "/engine-off";

/// Where the Start engine form posts.
const START: &str = "/start-engine";

/// Headers on every answer besides its content policy: no other page may
/// frame it, and nothing is guessed or referred.
pub const HEADERS: [(&str, &str); 3] = [
    ("x-frame-options", "DENY"),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "no-referrer"),
];

/// The content policy of every answer but the title bar: nothing loads from
/// anywhere else and no script runs.
const NO_SCRIPT: &str = "default-src 'self'; script-src 'none'; frame-ancestors 'none'; form-action 'self'; base-uri 'none'";

/// The title bar's content policy: its own script file, and Tauri's IPC
/// address, which carries its window commands; nothing else.
const TITLE_BAR: &str = "default-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost; frame-ancestors 'none'; form-action 'none'; base-uri 'none'";

/// Which of the window's two webviews asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Webview {
    /// The title bar's.
    TitleBar,
    /// The engine pages' (and so the Engine off screen's).
    Pages,
}

impl Webview {
    /// The webview with `label`; any but the bar's counts as the pages'.
    #[must_use]
    pub fn of(label: &str) -> Self {
        if label == title_bar::LABEL {
            Self::TitleBar
        } else {
            Self::Pages
        }
    }
}

/// One request to the app's pages, as the shell received it.
pub struct Request<'a> {
    /// The method, such as `GET`.
    pub method: &'a str,
    /// The path, such as `/engine-off`.
    pub path: &'a str,
    /// The `Origin` header, which a form post carries.
    pub origin: Option<&'a str>,
    /// The webview that asked.
    pub webview: Webview,
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
    /// Its content security policy.
    pub policy: &'static str,
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
            policy: NO_SCRIPT,
        }
    }
}

/// Answers `request` with the Engine off screen made from `off`, at `now`
/// on this computer's clock, or the title bar showing `bar`; gives the
/// action it asks for, if any.
#[must_use]
pub fn answer(
    request: &Request<'_>,
    off: &Off,
    bar: title_bar::View,
    now: jiff::Timestamp,
) -> (Answer, Option<Action>) {
    match (request.method, request.path) {
        ("GET", title_bar::PAGE) if request.webview == Webview::TitleBar => {
            let answer = match bar.render() {
                Ok(html) => Answer {
                    status: 200,
                    kind: "text/html; charset=utf-8",
                    body: html.into_bytes().into(),
                    location: None,
                    store: false,
                    policy: TITLE_BAR,
                },
                Err(error) => {
                    tracing::error!(%error, "the title bar could not be written");
                    Answer::text(500, "The title bar could not be written.")
                }
            };
            (answer, None)
        }
        ("GET", ENGINE_OFF) => {
            let zone = jiff::tz::TimeZone::system();
            let answer = match page::render(off, &zone, now) {
                Ok(html) => Answer {
                    status: 200,
                    kind: "text/html; charset=utf-8",
                    body: html.into_bytes().into(),
                    location: None,
                    store: false,
                    policy: NO_SCRIPT,
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
            if request.webview != Webview::Pages
                || request.origin != Some(crate::engine::Origin::Local.text().as_str())
            {
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
                policy: NO_SCRIPT,
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
                    policy: NO_SCRIPT,
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

    fn ask_from(
        webview: Webview,
        method: &'static str,
        path: &'static str,
        origin: Option<&'static str>,
    ) -> (Answer, Option<Action>) {
        answer(
            &Request {
                method,
                path,
                origin,
                webview,
            },
            &Off::default(),
            title_bar::View::default(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    fn ask(
        method: &'static str,
        path: &'static str,
        origin: Option<&'static str>,
    ) -> (Answer, Option<Action>) {
        ask_from(Webview::Pages, method, path, origin)
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
        assert!(screen.policy.contains("script-src 'none'"));
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

    // ADR 0019 (title bar): the bar is answered only to its own webview,
    // with a policy that runs only its own script file and reaches only
    // Tauri's IPC; the pages' webview gets no bar, and the bar no start.
    #[test]
    fn the_title_bar_is_only_for_its_own_webview() {
        assert_eq!(Webview::of(title_bar::LABEL), Webview::TitleBar);
        assert_eq!(Webview::of(title_bar::PAGES), Webview::Pages);
        let (bar, action) = ask_from(Webview::TitleBar, "GET", title_bar::PAGE, None);
        assert_eq!((bar.status, action, bar.store), (200, None, false));
        assert_eq!(
            bar.policy,
            concat!(
                "default-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost; ",
                "frame-ancestors 'none'; form-action 'none'; base-uri 'none'"
            )
        );
        let (script, _) = ask_from(Webview::TitleBar, "GET", "/static/title-bar.js", None);
        assert_eq!(
            (script.status, script.kind),
            (200, "text/javascript; charset=utf-8")
        );
        assert_eq!(ask("GET", title_bar::PAGE, None).0.status, 404);
        let (start, action) = ask_from(
            Webview::TitleBar,
            "POST",
            "/start-engine",
            Some("http://analyte.localhost"),
        );
        assert_eq!((start.status, action), (403, None));
    }
}
