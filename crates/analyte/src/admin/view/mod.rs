//! Holds the old pages' layout until PR b6 (their head, menu bar and `admin.css`), and the tabs, tones and table columns every page shares.
//!
//! Restyled pages are Askama templates with a [`frame::Frame`]; the pages not
//! restyled yet are built here as markup and load the old stylesheet, which is
//! served from the binary at a same-origin path, so the `default-src 'self'`
//! content security policy allows it unchanged. No page uses inline styles,
//! scripts, fonts or external URLs.
pub(super) mod assets;
pub(super) mod format;
pub(super) mod frame;
pub(super) mod icon;
pub(super) mod report;

use super::markup::Markup;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

/// How long a restyled page may read before it answers with its own error
/// page: below the limit on every admin request, so a slow store still
/// gets a page in the new look. Pages that only read leave nothing behind when
/// stopped; a store request already queued may still run, and its answer is
/// dropped. [`format::SLOW_READ`] says this time in words.
pub(super) const READ_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// Where the old pages' stylesheet is served (see [`assets`]).
pub(super) const STYLESHEET: &str = "/static/admin.css";
#[cfg(test)]
const CSS: &str = include_str!("../../../assets/admin.css");

/// One entry of the menu. The first four are the main tabs; the rest sit under "More".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Overview,
    NeedsReview,
    Machines,
    /// The Lab apps page under Setup; the restyled sidebar lists it, the old menu does not.
    LabApps,
    Setup,
    MachineOrders,
    CancelledByMachine,
    Deliveries,
    DataKeeping,
}

impl Tab {
    /// The main tabs, always visible.
    const MAIN: [Self; 4] = [
        Self::Overview,
        Self::NeedsReview,
        Self::Machines,
        Self::Setup,
    ];
    /// The pages under "More".
    const MORE: [Self; 4] = [
        Self::MachineOrders,
        Self::CancelledByMachine,
        Self::Deliveries,
        Self::DataKeeping,
    ];

    /// The page's path; these stay as they were, so bookmarks and docs keep working.
    pub(super) const fn href(self) -> &'static str {
        match self {
            Self::Overview => "/",
            Self::NeedsReview => "/parked",
            Self::Machines => "/machines",
            Self::LabApps => crate::admin::setup::LAB_APPS,
            Self::Setup => "/setup",
            Self::MachineOrders => "/orders",
            Self::CancelledByMachine => "/analyzer-cancelled",
            Self::Deliveries => "/deliveries",
            Self::DataKeeping => "/retention",
        }
    }

    /// The menu label, which is also the page's heading.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::NeedsReview => "Needs review",
            Self::Machines => "Machines",
            Self::LabApps => "Lab apps",
            Self::Setup => "Setup",
            Self::MachineOrders => "Machine orders",
            Self::CancelledByMachine => "Cancelled by machine",
            Self::Deliveries => "Deliveries",
            Self::DataKeeping => "Data keeping",
        }
    }

    fn link(self, current: Option<Self>) -> Markup {
        Markup::new()
            .markup("<li><a href=\"")
            .markup(self.href())
            .markup(if current == Some(self) {
                "\" aria-current=\"page\">"
            } else {
                "\">"
            })
            .markup(self.label())
            .markup("</a></li>")
    }
}

/// The header bar: product name, the four main tabs and the "More" dropdown,
/// with the current page marked. The dropdown is a `<details>` element, so it
/// opens by mouse or keyboard without any script.
fn bar(current: Option<Tab>) -> Markup {
    let mut html = Markup::new().markup(concat!(
        "<header class=\"bar\"><div class=\"bar-inner\"><a class=\"brand\" href=\"/\">",
        "<img class=\"mark\" src=\"/static/icons/analyte-app-icon.svg\" alt=\"\" width=\"24\" height=\"24\">Analyte</a>",
        "<nav aria-label=\"Admin pages\"><ul class=\"tabs\">",
    ));
    for tab in Tab::MAIN {
        html.push_fragment(&tab.link(current));
    }
    html.push_markup(if current.is_some_and(|tab| Tab::MORE.contains(&tab)) {
        "<li class=\"more\"><details><summary class=\"current\">More</summary><ul class=\"menu\">"
    } else {
        "<li class=\"more\"><details><summary>More</summary><ul class=\"menu\">"
    });
    for tab in Tab::MORE {
        html.push_fragment(&tab.link(current));
    }
    html.markup("</ul></details></li></ul></nav></div></header>")
}

/// Closes what [`page`] opened.
pub(super) const END: &str = "</main></body></html>";

/// Which header a page gets.
#[derive(Clone, Copy)]
enum Header {
    /// The menu, with this page's tab marked when it belongs to one.
    Menu(Option<Tab>),
    /// The login page: no menu, since nothing else opens before login.
    Login,
}

/// Opens a page under `tab` with the header bar and its title as the main heading; close it with [`END`].
pub(super) fn page(tab: Tab, title: &str) -> Markup {
    head(title, Header::Menu(Some(tab)))
}

/// The login page: no navigation, since nothing else opens before login.
pub(super) fn login(title: &str) -> Markup {
    head(title, Header::Login)
}

/// A whole page under `tab` that only reports `message` under `title`; the message is written for people.
pub(super) fn message(tab: Tab, title: &str, message: impl std::fmt::Display) -> Markup {
    page(tab, title)
        .markup("<p class=\"notice bad\">")
        .text(message)
        .markup("</p>")
        .markup(END)
}

/// A whole page for something that went wrong: a plain `sentence` saying what it
/// means, and the exact technical `detail` kept small for support.
pub(super) fn problem(
    tab: Option<Tab>,
    title: &str,
    sentence: &'static str,
    detail: impl std::fmt::Display,
) -> Markup {
    head(title, Header::Menu(tab))
        .markup("<p class=\"notice bad\">")
        .markup(sentence)
        .markup("</p><p class=\"muted\">Details for support: <code>")
        .text(detail)
        .markup("</code></p>")
        .markup(END)
}

/// [`problem`] as a response with `status`, for refusals that have no page of their own.
pub(super) fn refusal(
    status: StatusCode,
    title: &str,
    sentence: &'static str,
    detail: impl std::fmt::Display,
) -> Response {
    (status, Html(problem(None, title, sentence, detail))).into_response()
}

fn head(title: &str, header: Header) -> Markup {
    let mut html = Markup::new().markup(concat!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    ));
    html.push_fragment(
        &Markup::new()
            .markup("<title>")
            .text(title)
            .markup(" · Analyte</title>"),
    );
    for icon in &assets::FAVICONS {
        html.push_fragment(
            &Markup::new()
                .markup("<link rel=\"icon\" type=\"image/svg+xml\" sizes=\"")
                .markup(icon.sizes)
                .markup("\" href=\"/static/")
                .markup(icon.path)
                .markup("\">"),
        );
    }
    html.push_fragment(
        &Markup::new()
            .markup("<link rel=\"stylesheet\" href=\"")
            .markup(STYLESHEET)
            .markup("\"></head>"),
    );
    match header {
        Header::Menu(current) => {
            html.push_markup("<body>");
            html.push_fragment(&bar(current));
        }
        Header::Login => html.push_markup("<body class=\"login\">"),
    }
    html.markup("<main><h1>").text(title).markup("</h1>")
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
    /// The design system's name for the tone, as in `status--good` or `tone-warn`.
    pub(super) const fn class(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Attention => "warn",
            Self::Problem => "bad",
            Self::Neutral => "off",
        }
    }

    /// The icon that carries the tone, so status is never colour alone.
    pub(super) const fn icon(self) -> icon::Icon {
        match self {
            Self::Good => icon::Icon::Ok,
            Self::Attention => icon::Icon::Warning,
            Self::Problem => icon::Icon::Problem,
            Self::Neutral => icon::Icon::Waiting,
        }
    }

    /// The word a to-do line shows above its title.
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Good => "Good",
            Self::Attention => "Warning",
            Self::Problem => "Problem",
            Self::Neutral => "Note",
        }
    }

    const fn open(self) -> &'static str {
        match self {
            Self::Good => "<span class=\"pill ok\">",
            Self::Attention => "<span class=\"pill warn\">",
            Self::Problem => "<span class=\"pill bad\">",
            Self::Neutral => "<span class=\"pill\">",
        }
    }
}

/// One column of a restyled table: its heading and whether it holds numbers,
/// which line up on the right.
pub(super) struct Column {
    pub(super) label: &'static str,
    pub(super) right: bool,
}

impl Column {
    /// A column of words, aligned left.
    pub(super) const fn text(label: &'static str) -> Self {
        Self {
            label,
            right: false,
        }
    }

    /// A column of numbers, aligned right.
    pub(super) const fn number(label: &'static str) -> Self {
        Self { label, right: true }
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

/// A count of things that may need attention: a plain number, red only when above zero.
pub(super) fn count(value: u64) -> Markup {
    Markup::new()
        .markup(if value == 0 {
            "<span class=\"count\">"
        } else {
            "<span class=\"count over\">"
        })
        .text(value)
        .markup("</span>")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner request 2026-11-19 (readable admin pages): every page loads the one
    // same-origin stylesheet, and the login page has no navigation.
    #[test]
    fn pages_link_the_stylesheet_and_close_cleanly() {
        let page = page(Tab::Machines, "Machines").markup(END);
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
    }

    // Owner request 2026-11-19 (lab-manager admin pages, item 6): four main tabs,
    // a "More" dropdown with the other four pages, the current page marked, and
    // the old paths kept.
    #[test]
    fn menu_has_four_tabs_and_more_and_marks_the_current_page() {
        let main = [
            ("/", "Overview"),
            ("/parked", "Needs review"),
            ("/machines", "Machines"),
            ("/setup", "Setup"),
        ];
        let more = [
            ("/orders", "Machine orders"),
            ("/analyzer-cancelled", "Cancelled by machine"),
            ("/deliveries", "Deliveries"),
            ("/retention", "Data keeping"),
        ];
        let overview = page(Tab::Overview, "Overview");
        let (tabs, menu) = overview
            .split_once("<li class=\"more\">")
            .unwrap_or_default();
        for (href, label) in main {
            assert!(tabs.contains(&format!("href=\"{href}\"")), "{label}");
            assert!(tabs.contains(&format!(">{label}</a>")), "{label}");
        }
        for (href, label) in more {
            assert!(
                menu.contains(&format!("<a href=\"{href}\">{label}</a>")),
                "{label}"
            );
        }
        assert!(overview.contains("<a href=\"/\" aria-current=\"page\">Overview</a>"));
        assert_eq!(overview.matches("aria-current").count(), 1);
        assert!(overview.contains("<details><summary>More</summary>"));
        let deliveries = page(Tab::Deliveries, "Deliveries");
        assert!(
            deliveries.contains("<a href=\"/deliveries\" aria-current=\"page\">Deliveries</a>")
        );
        assert!(deliveries.contains("<summary class=\"current\">More</summary>"));
        let refused = problem(None, "Not found", "That page does not exist.", "x");
        assert!(refused.contains("<details><summary>More</summary>"));
        assert!(!refused.contains("aria-current"));
    }

    // Owner request 2026-11-19 (item 8): counts are plain numbers, red only above zero.
    #[test]
    fn counts_are_plain_and_red_only_above_zero() {
        assert_eq!(&*count(0), "<span class=\"count\">0</span>");
        assert_eq!(&*count(1), "<span class=\"count over\">1</span>");
        assert_eq!(
            &*count(u64::MAX),
            "<span class=\"count over\">18446744073709551615</span>"
        );
        assert!(CSS.contains(".count.over"));
        assert!(!count(3).contains("pill"));
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

    /// Every file under `folder`, with its path relative to the crate.
    fn files(folder: &str) -> Result<Vec<(String, String)>, std::io::Error> {
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        let mut folders = vec![crate_root.join(folder)];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(folder)? {
                let path = entry?.path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    let name = path
                        .strip_prefix(crate_root)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    found.push((name, std::fs::read_to_string(&path)?));
                }
            }
        }
        Ok(found)
    }

    // ADR 0018: the restyled pages keep the strict CSP and escaping. No template
    // holds an inline style or script, or turns escaping off; each starts with a
    // comment saying what it holds.
    #[test]
    fn templates_hold_no_inline_style_script_or_unescaped_output() -> Result<(), std::io::Error> {
        let templates = files("templates")?;
        assert!(templates.len() >= 8, "{}", templates.len());
        for (name, text) in templates {
            for forbidden in [
                "style=",
                "<style",
                "<script",
                "|safe",
                "| safe",
                "escape = \"none\"",
            ] {
                assert!(!text.contains(forbidden), "{name} contains {forbidden}");
            }
            assert!(text.starts_with("{#- "), "{name} lacks its opening comment");
        }
        Ok(())
    }

    /// The restyled pages' additions to the design system.
    const APP: &str = include_str!("../../../assets/app.css");

    // ADR 0018: app.css adds to the design system with semantic tokens only: no
    // primitives, no raw colours, nothing loaded from elsewhere.
    #[test]
    fn app_css_uses_semantic_tokens_only() {
        assert!(APP.starts_with("/* Analyte's additions"));
        for forbidden in [
            "--p-", "#", "rgb(", "rgba(", "hsl(", "@import", "url(", "http:", "https:",
        ] {
            assert!(!APP.contains(forbidden), "app.css contains {forbidden}");
        }
    }

    // Owner request 2026-11-19 (PR #273): moving between restyled pages
    // cross-fades with CSS only, in the design's base duration and easing; the
    // sidebar keeps its own name so it stays still; reduced motion turns it off.
    #[test]
    fn page_changes_cross_fade_unless_motion_is_reduced() {
        for rule in [
            "@view-transition { navigation: auto; }",
            "animation-duration: var(--dur-base);",
            "animation-timing-function: var(--ease-out);",
            ".sidebar { view-transition-name: sidebar; }",
        ] {
            assert!(APP.contains(rule), "app.css lacks {rule}");
        }
        let reduced = APP
            .split_once("@media (prefers-reduced-motion: reduce) {")
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        assert!(reduced.contains("@view-transition { navigation: none; }"));
        assert!(reduced.contains("::view-transition-old(*)"));
        assert!(reduced.contains("animation: none;"));
    }
}
