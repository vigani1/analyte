//! Decides where the window may go: only the engine's admin pages and the app's own Engine off screen; Windows sign-in is the app's to run; other web links open in the default browser.
//!
//! The window carries the admin session, so it never shows another site. A
//! link to the web goes to the browser, where it has no session; anything
//! else (files, other programs' schemes) is refused.
//!
//! `WebView2` sends a navigation's request before the app's check can cancel
//! it, so the check alone would still let a link reach another site from the
//! window, with the window's cookies. [`PAGES_SCRIPT`] therefore turns a
//! click on a link to another origin into a new-window request, which the
//! app answers before anything is sent.
use crate::engine::Origin;
use crate::sign_in::{self, Trigger};

/// The script the pages' webview runs at the start of every page, before
/// the page's own (`crates/app/assets/pages.js`): a click on a link to
/// another origin asks for a new window instead, so [`decide`] answers it
/// before `WebView2` sends a request. Without it, a link to another port on
/// 127.0.0.1 reached that port with the admin session's cookie, because
/// cookies ignore the port.
pub const PAGES_SCRIPT: &str = include_str!("../assets/pages.js");

/// What the window does with an address it is asked to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Show it in the window.
    Show,
    /// Stay put and sign in with Windows through the engine's pipe, then
    /// open `next` (ADR 0021 §6): the engine's `/login/windows` without a code.
    WindowsSignIn {
        /// The page to land on, as the address gave it; the engine checks it.
        next: String,
        /// Whether a person or the engine's refresh asked.
        trigger: Trigger,
    },
    /// Leave the window as it is and open the address in the default browser.
    OpenOutside,
    /// Do nothing.
    Refuse,
}

/// Where `url` may be shown, given the engine's origin.
#[must_use]
pub fn decide(url: &url::Url, engine: Origin) -> Decision {
    if matches!(engine, Origin::Engine(_)) && engine.holds(url) && url.path() == sign_in::PATH {
        windows_sign_in(url)
    } else if engine.holds(url) || Origin::Local.holds(url) {
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

/// `/login/windows`: with a code the engine redeems it, so it shows;
/// without one the app signs in. The engine's refresh and the window's start
/// name the page to return to; the sign-in page's button names none, so a
/// missing `next` is a person's click and lands on the Overview.
fn windows_sign_in(url: &url::Url) -> Decision {
    let mut next = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => return Decision::Show,
            "next" if next.is_none() => next = Some(value.into_owned()),
            _ => {}
        }
    }
    match next {
        Some(next) => Decision::WindowsSignIn {
            next,
            trigger: Trigger::Automatic,
        },
        None => Decision::WindowsSignIn {
            next: "/".to_owned(),
            trigger: Trigger::Person,
        },
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

    // ADR 0021 §6 and #296: the engine's refresh to /login/windows?next=<page>
    // and the sign-in page's button (no next) are the app's to run; a code
    // is the engine's to redeem, so it shows.
    #[test]
    fn windows_sign_in_without_a_code_is_the_apps() {
        let automatic = |next: &str| Decision::WindowsSignIn {
            next: next.to_owned(),
            trigger: Trigger::Automatic,
        };
        assert_eq!(
            decision("http://127.0.0.1:8080/login/windows?next=%2Fparked%3Fafter%3D5"),
            automatic("/parked?after=5")
        );
        assert_eq!(
            decision("http://127.0.0.1:8080/login/windows?next=/"),
            automatic("/")
        );
        assert_eq!(
            decision("http://127.0.0.1:8080/login/windows?next=/a&next=/b"),
            automatic("/a")
        );
        assert_eq!(
            decision("http://127.0.0.1:8080/login/windows"),
            Decision::WindowsSignIn {
                next: "/".to_owned(),
                trigger: Trigger::Person,
            }
        );
        for shown in [
            "http://127.0.0.1:8080/login/windows?code=ab%2Bc%3D",
            "http://127.0.0.1:8080/login/windows?next=/&code=x",
            "http://127.0.0.1:8080/login",
            "http://127.0.0.1:8080/login?windows=not-allowed",
            "http://127.0.0.1:8080/login/windowsx",
        ] {
            assert_eq!(decision(shown), Decision::Show, "{shown}");
        }
        // Only the engine's own origin: elsewhere the address is a web link.
        assert_eq!(
            decision("http://127.0.0.1:8081/login/windows"),
            Decision::OpenOutside
        );
        let local = url::Url::parse("http://analyte.localhost/login/windows").expect("an address");
        assert_eq!(decide(&local, Origin::Local), Decision::Show);
    }

    // App test round A, item 4: WebView2 sent a link's request, with the
    // session cookie, before the navigation check cancelled it. The pages'
    // script sends every click on an http(s) link to another origin to a new
    // window, which the app decides on before any request, and leaves the
    // page's own links, downloads and other windows' links to the browser
    // and the check. Its behaviour is shown live in the PR; this guards the
    // rule's parts.
    #[test]
    fn the_pages_script_sends_links_elsewhere_to_a_new_window() {
        let script = PAGES_SCRIPT;
        assert!(script.starts_with("// pages.js: "), "its opening comment");
        assert!(script.contains("'use strict';"));
        for part in [
            "addEventListener('click', function (event) {",
            "if (event.defaultPrevented || event.button !== 0",
            "closest('a[href], area[href]')",
            "link.hasAttribute('download')",
            "if (target !== '_self' && target !== '_top' && target !== '_parent') return;",
            "new URL(link.getAttribute('href'), document.baseURI)",
            "if (url.origin === location.origin || (url.protocol !== 'http:' && url.protocol !== 'https:')) return;",
            "event.preventDefault();
  window.open(url.href, '_blank', 'noopener');",
        ] {
            assert!(script.contains(part), "the script lacks {part}");
        }
        assert_eq!(script.matches("addEventListener(").count(), 1);
        for forbidden in [
            "eval(",
            "new Function",
            "innerHTML",
            "document.write",
            "fetch(",
            "__TAURI",
        ] {
            assert!(!script.contains(forbidden), "the script uses {forbidden}");
        }
    }
}
