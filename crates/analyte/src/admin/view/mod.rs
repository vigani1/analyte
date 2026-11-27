//! Holds what every page shares: the tabs, tones and table columns, and the read budget; the frames, formatting, icons, list paging and reports live in the modules below.
//!
//! Every page is an Askama template in `templates/` with one view struct. Its
//! files are served from the binary at same-origin paths, so the
//! `default-src 'self'` content security policy allows them unchanged. No page
//! uses inline styles, scripts, fonts or external URLs.
pub(super) mod assets;
pub(super) mod format;
pub(super) mod frame;
pub(super) mod icon;
pub(super) mod list;
pub(super) mod parts;
pub(super) mod refusal;
pub(super) mod report;

/// How long a page may read before it answers with its own error page:
/// below the limit on every admin request, so a slow store still gets a page
/// that says what could not be read. Pages that only read leave nothing behind when
/// stopped; a store request already queued may still run, and its answer is
/// dropped. [`format::SLOW_READ`] says this time in words.
pub(super) const READ_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// One page of the sidebar: the main links, and the rest under "More".
use analyte_i18n::text as tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Overview,
    /// What Analyte stored, newest first.
    Activity,
    NeedsReview,
    Machines,
    /// The Lab apps page.
    LabApps,
    Setup,
    MachineOrders,
    CancelledByMachine,
    Deliveries,
    DataKeeping,
    /// What Analyte runs with, read-only.
    Settings,
}

impl Tab {
    /// The page's path; these stay as they were, so bookmarks and docs keep working.
    pub(super) const fn href(self) -> &'static str {
        match self {
            Self::Overview => "/",
            Self::Activity => "/activity",
            Self::NeedsReview => "/parked",
            Self::Machines => "/machines",
            Self::LabApps => crate::admin::setup::LAB_APPS,
            Self::Setup => "/setup",
            Self::MachineOrders => "/orders",
            Self::CancelledByMachine => "/analyzer-cancelled",
            Self::Deliveries => "/deliveries",
            Self::DataKeeping => "/retention",
            Self::Settings => "/settings",
        }
    }

    /// The sidebar label, which is also the page's heading.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Overview => tr("admin-overview"),
            Self::Activity => tr("admin-activity"),
            Self::NeedsReview => tr("admin-needs-review"),
            Self::Machines => tr("admin-machines"),
            Self::LabApps => tr("admin-lab-apps-2"),
            Self::Setup => tr("admin-setup"),
            Self::MachineOrders => tr("admin-machine-orders"),
            Self::CancelledByMachine => tr("admin-cancelled-by-machine"),
            Self::Deliveries => tr("admin-deliveries"),
            Self::DataKeeping => tr("admin-data-keeping"),
            Self::Settings => tr("admin-settings"),
        }
    }

    /// The link back to the page, at the foot of a page that only reports.
    pub(super) fn back(self) -> &'static str {
        match self {
            Self::Overview => tr("admin-back-to-overview"),
            Self::Activity => tr("admin-back-to-activity"),
            Self::NeedsReview => tr("admin-back-to-needs-review"),
            Self::Machines => tr("admin-back-to-machines"),
            Self::LabApps => tr("admin-back-to-lab-apps"),
            Self::Setup => tr("admin-back-to-setup"),
            Self::MachineOrders => tr("admin-back-to-machine-orders"),
            Self::CancelledByMachine => tr("admin-back-to-cancelled-by-machine"),
            Self::Deliveries => tr("admin-back-to-deliveries"),
            Self::DataKeeping => tr("admin-back-to-data-keeping"),
            Self::Settings => tr("admin-back-to-settings"),
        }
    }
}

/// How a status reads: its colour, icon and word.
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
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Good => tr("admin-good"),
            Self::Attention => tr("admin-warning"),
            Self::Problem => tr("admin-problem"),
            Self::Neutral => tr("admin-note"),
        }
    }
}

/// One column of a table: its heading, whether it holds numbers,
/// which line up on the right, and whether its heading is for screen readers
/// only, as over a column of links.
pub(super) struct Column {
    pub(super) label: &'static str,
    pub(super) right: bool,
    pub(super) hidden: bool,
}

impl Column {
    /// A column of words, aligned left.
    pub(super) const fn text(label: &'static str) -> Self {
        Self {
            label,
            right: false,
            hidden: false,
        }
    }

    /// A column of numbers, aligned right.
    pub(super) const fn number(label: &'static str) -> Self {
        Self {
            label,
            right: true,
            hidden: false,
        }
    }

    /// A column of links, aligned right, whose heading only screen readers read.
    pub(super) const fn action(label: &'static str) -> Self {
        Self {
            label,
            right: true,
            hidden: true,
        }
    }
}

#[cfg(test)]
mod tests {
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

    /// The one script tag a template may hold: the live-updates file, in the
    /// signed-in frame's head.
    const LIVE_SCRIPT: &str = "<script src=\"{{ frame.assets }}live.js\" defer></script>";

    // ADR 0018: every page keeps the strict CSP and escaping. No template
    // holds an inline style or script, or turns escaping off; each starts with a
    // comment saying what it holds. The only script is the live-updates file,
    // loaded once, by the signed-in frame (owner decision 2026-11-20, PR c2).
    #[test]
    fn templates_hold_no_inline_style_script_or_unescaped_output() -> Result<(), std::io::Error> {
        let templates = files("templates")?;
        assert!(templates.len() >= 8, "{}", templates.len());
        let mut loaded = Vec::new();
        for (name, text) in templates {
            if text.contains(LIVE_SCRIPT) {
                loaded.push(name.clone());
            }
            let text = text.replacen(LIVE_SCRIPT, "", 1);
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
        assert_eq!(
            loaded,
            [std::path::Path::new("templates")
                .join("layout")
                .join("base.html")
                .display()
                .to_string()]
        );
        Ok(())
    }

    /// Analyte's additions to the design system.
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

    // Owner request 2026-11-19 (PR #273): moving between pages
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
