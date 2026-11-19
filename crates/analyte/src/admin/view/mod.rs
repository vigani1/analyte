//! Lays out every admin page: the document head, the header bar with navigation, and the one stylesheet.
//!
//! The stylesheet is served from the binary at a same-origin path, so the
//! `default-src 'self'` content security policy allows it unchanged. Pages use no
//! inline styles, scripts, fonts or external URLs.
pub(super) mod plain;

use super::markup::Markup;
use axum::{
    http::header,
    response::{IntoResponse, Response},
};

/// Where the stylesheet is served.
pub(super) const STYLESHEET: &str = "/static/admin.css";
const CSS: &str = include_str!("style.css");

/// The header bar: product name and the page links.
const BAR: &str = concat!(
    "<header><a class=\"brand\" href=\"/\">Analyte</a><nav aria-label=\"Admin pages\">",
    "<a href=\"/\">Overview</a><a href=\"/machines\">Machines</a>",
    "<a href=\"/parked\">Parked items</a><a href=\"/orders\">Machine orders</a>",
    "<a href=\"/analyzer-cancelled\">Analyzer cancellations</a>",
    "<a href=\"/deliveries\">Deliveries</a><a href=\"/retention\">Retention and holds</a>",
    "<a href=\"/setup\">Setup</a></nav></header>",
);

/// Closes what [`page`] opened.
pub(super) const END: &str = "</main></body></html>";

/// How often a page reloads itself, as the `content` of a refresh `<meta>`.
#[derive(Clone, Copy)]
pub(super) enum Refresh {
    /// The live connection test.
    EveryTwoSeconds,
    /// After "Restart now": back to the setup page once the service is up again.
    SetupAfterRestart,
}

impl Refresh {
    const fn content(self) -> &'static str {
        match self {
            Self::EveryTwoSeconds => "2",
            Self::SetupAfterRestart => "15;url=/setup",
        }
    }
}

/// Opens a page with the header bar and its title as the main heading; close it with [`END`].
pub(super) fn page(title: &str) -> Markup {
    head(title, None, true)
}

/// Like [`page`], and the browser reloads it as `refresh` says.
pub(super) fn refreshing(title: &str, refresh: Refresh) -> Markup {
    head(title, Some(refresh), true)
}

/// The login page: no navigation, since nothing else opens before login.
pub(super) fn login(title: &str) -> Markup {
    head(title, None, false)
}

/// A whole page that only reports `message` under `title`.
pub(super) fn message(title: &str, message: impl std::fmt::Display) -> Markup {
    page(title)
        .markup("<p class=\"notice bad\">")
        .text(message)
        .markup("</p>")
        .markup(END)
}

fn head(title: &str, refresh: Option<Refresh>, navigation: bool) -> Markup {
    let mut html = Markup::new().markup(concat!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    ));
    if let Some(refresh) = refresh {
        html.push_markup("<meta http-equiv=\"refresh\" content=\"");
        html.push_markup(refresh.content());
        html.push_markup("\">");
    }
    html.push_fragment(
        &Markup::new()
            .markup("<title>")
            .text(title)
            .markup(" · Analyte</title><link rel=\"stylesheet\" href=\"")
            .markup(STYLESHEET)
            .markup("\"></head>"),
    );
    html.push_markup(if navigation {
        "<body>"
    } else {
        "<body class=\"login\">"
    });
    if navigation {
        html.push_markup(BAR);
    }
    html.markup("<main><h1>").text(title).markup("</h1>")
}

/// GET /static/admin.css. It holds no data, so the login page may load it too.
pub(super) async fn stylesheet() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS).into_response()
}

/// A coloured status pill; `tone` picks the colour.
pub(super) fn pill(tone: Tone, label: &str) -> Markup {
    Markup::new()
        .markup(tone.open())
        .text(label)
        .markup("</span>")
}

/// The colour of a status pill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    /// Working as expected: connected, port open, ready.
    Good,
    /// Needs a look soon: waiting, paused, blocked.
    Attention,
    /// Something failed or is refused.
    Problem,
    /// Off or not set up; nothing is wrong by itself.
    Neutral,
}

impl Tone {
    const fn open(self) -> &'static str {
        match self {
            Self::Good => "<span class=\"pill ok\">",
            Self::Attention => "<span class=\"pill warn\">",
            Self::Problem => "<span class=\"pill bad\">",
            Self::Neutral => "<span class=\"pill\">",
        }
    }
}

/// A key and value row for a `<dl>`; `exact` is the technical name shown on hover.
pub(super) fn row(name: &str, exact: Option<&str>, value: &Markup) -> Markup {
    let mut html = Markup::new().markup("<dt");
    if let Some(exact) = exact {
        html.push_markup(" title=\"");
        html.push_text(exact);
        html.push_markup("\"");
    }
    html.markup(">")
        .text(name)
        .markup("</dt><dd>")
        .fragment(value)
        .markup("</dd>")
}

/// Plain text whose exact technical value shows on hover.
pub(super) fn exact(shown: &str, technical: &str) -> Markup {
    Markup::new()
        .markup("<span title=\"")
        .text(technical)
        .markup("\">")
        .text(shown)
        .markup("</span>")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner request 2026-11-19 (readable admin pages): every page loads the one
    // same-origin stylesheet, and the login page has no navigation.
    #[test]
    fn pages_link_the_stylesheet_and_close_cleanly() {
        let page = page("Machines").markup(END);
        assert!(
            page.starts_with("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">")
        );
        assert!(page.contains("<link rel=\"stylesheet\" href=\"/static/admin.css\"></head>"));
        assert!(page.contains("<nav aria-label=\"Admin pages\">"));
        assert!(page.contains("<main><h1>Machines</h1>"));
        assert_eq!(page.matches("</html>").count(), 1);
        let login = login("Analyte admin");
        assert!(login.contains("href=\"/static/admin.css\""));
        assert!(!login.contains("<nav"));
        let live = refreshing("Connection test", Refresh::EveryTwoSeconds);
        assert!(live.contains("<meta http-equiv=\"refresh\" content=\"2\">"));
    }

    // The stylesheet stays inside the CSP: no imports, fonts or external URLs.
    #[test]
    fn stylesheet_loads_nothing_else() {
        for forbidden in [
            "@import",
            "url(",
            "@font-face",
            "http:",
            "https:",
            "expression(",
        ] {
            assert!(!CSS.contains(forbidden), "stylesheet contains {forbidden}");
        }
    }

    #[test]
    fn hovered_exact_values_are_escaped() {
        let html = exact("Yes", "fresh=\"true\" <x>");
        assert_eq!(
            &*html,
            "<span title=\"fresh=&quot;true&quot; &lt;x&gt;\">Yes</span>"
        );
        let html = row(
            "Order list up to date",
            Some("Cache fresh and trusted"),
            &html,
        );
        assert!(
            html.starts_with(
                "<dt title=\"Cache fresh and trusted\">Order list up to date</dt><dd>"
            )
        );
        assert_eq!(
            &*pill(Tone::Good, "Connected"),
            "<span class=\"pill ok\">Connected</span>"
        );
    }
}
