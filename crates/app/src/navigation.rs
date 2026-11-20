//! Decides where the window may go: only the engine's admin pages and the app's own Engine off screen; other web links open in the default browser.
//!
//! The window carries the admin session, so it never shows another site. A
//! link to the web goes to the browser, where it has no session; anything
//! else (files, other programs' schemes) is refused.
use crate::engine::Origin;

/// What the window does with an address it is asked to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Show it in the window.
    Show,
    /// Leave the window as it is and open the address in the default browser.
    OpenOutside,
    /// Do nothing.
    Refuse,
}

/// Where `url` may be shown, given the engine's origin.
#[must_use]
pub fn decide(url: &url::Url, engine: Origin) -> Decision {
    if engine.holds(url) || Origin::Local.holds(url) {
        Decision::Show
    } else if matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
    {
        Decision::OpenOutside
    } else {
        Decision::Refuse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(address: &str) -> Decision {
        let url = url::Url::parse(address).expect("a test address");
        decide(&url, Origin::Engine(8080))
    }

    // ADR 0019 (window): only the engine's origin and the app's own pages
    // show in the window; the engine's origin is exact, as its Host check is.
    #[test]
    fn only_the_engine_and_the_app_show_in_the_window() {
        for shown in [
            "http://127.0.0.1:8080/",
            "http://127.0.0.1:8080/parked/7?x=1#top",
            "http://analyte.localhost/engine-off",
            "http://analyte.localhost:80/static/analyte.css",
        ] {
            assert_eq!(decision(shown), Decision::Show, "{shown}");
        }
        for outside in [
            "https://127.0.0.1:8080/",
            "http://127.0.0.1:8081/",
            "http://localhost:8080/",
            "http://[::1]:8080/",
            "https://github.com/vigani1/analyte",
            "http://analyte.localhost:8080/",
            "https://analyte.localhost/",
        ] {
            assert_eq!(decision(outside), Decision::OpenOutside, "{outside}");
        }
        for refused in [
            "http://user:secret@127.0.0.1:8080/",
            "file:///C:/Windows/win.ini",
            "about:blank",
            "javascript:alert(1)",
            "ms-settings:display",
            "mailto:lab@example.org",
            "data:text/html,hello",
        ] {
            assert_eq!(decision(refused), Decision::Refuse, "{refused}");
        }
    }
}
