//! The app's own title bar: the Aperture mark and "Analyte", a dot with the engine's state and its word, and the minimize, maximize or restore, and close buttons; where it sits in the window, and what the shell tells it.
//!
//! The bar is the app's own page (`http://analyte.localhost/title-bar`) in a
//! webview of its own above the engine's pages (ADR 0019). It is the only
//! webview allowed any Tauri command, and only the window commands it needs;
//! the engine's pages stay without any. The bar never asks the app anything:
//! the shell pushes what it shows with [`View::script`].
use crate::watch::{Shown, Tray};
use askama::Template as _;

/// The bar's address on the app's own origin.
pub const PAGE: &str = "/title-bar";

/// The bar's webview label, the only one the `title-bar` capability names.
pub const LABEL: &str = "title-bar";

/// The engine pages' webview label, which no capability names.
pub const PAGES: &str = "pages";

/// The bar's height in logical pixels: a Windows 11 caption, whose buttons
/// are 46 by 32.
pub const HEIGHT: f64 = 32.0;

/// Whether the window fills the screen, which names the middle button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sizing {
    /// A normal window: the button maximizes.
    #[default]
    Normal,
    /// A maximized window: the button restores.
    Maximized,
}

impl Sizing {
    /// The sizing of a window that is (or is not) maximized.
    #[must_use]
    pub const fn of(maximized: bool) -> Self {
        if maximized {
            Self::Maximized
        } else {
            Self::Normal
        }
    }

    /// The middle button's name, as Windows says it.
    #[must_use]
    pub const fn button(self) -> &'static str {
        match self {
            Self::Normal => "Maximize",
            Self::Maximized => "Restore",
        }
    }

    /// The bar's `data-sizing` value, which picks the button's drawing.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Maximized => "maximized",
        }
    }
}

/// What the bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    /// The engine's state, as the tray shows it.
    pub shown: Shown,
    /// Whether the window is maximized.
    pub sizing: Sizing,
}

impl Default for View {
    fn default() -> Self {
        Self {
            shown: Shown::Checking,
            sizing: Sizing::Normal,
        }
    }
}

impl View {
    /// The dot's classes: the design's status dot in the tray's four states
    /// (good, attention, problem, off), standing still in the bar.
    #[must_use]
    pub const fn dot(self) -> &'static str {
        match self.shown.tray() {
            Tray::Good => "live-dot live-dot--still",
            Tray::Attention => "live-dot live-dot--still live-dot--warn",
            Tray::Problem => "live-dot live-dot--still live-dot--bad",
            Tray::Off => "live-dot live-dot--still live-dot--off",
        }
    }

    /// The state's word, shown on hover: the tray's own words.
    #[must_use]
    pub const fn word(self) -> &'static str {
        self.shown.words()
    }

    /// The dot's accessible name.
    #[must_use]
    pub fn label(self) -> String {
        format!("Engine: {}", self.word())
    }

    /// The script the shell runs in the bar to show this view. It passes
    /// only the words above, as JSON, to the bar's own `analyteTitleBar`; a
    /// bar still loading ignores it and is brought up to date when loaded.
    #[must_use]
    pub fn script(self) -> String {
        let view = serde_json::json!({
            "dot": self.dot(),
            "word": self.word(),
            "label": self.label(),
            "sizing": self.sizing.word(),
            "button": self.sizing.button(),
        });
        format!("window.analyteTitleBar && window.analyteTitleBar({view});")
    }

    /// The bar's page showing this view.
    ///
    /// # Errors
    ///
    /// When the template cannot be written.
    pub fn render(self) -> Result<String, askama::Error> {
        Page {
            dot: self.dot(),
            word: self.word(),
            label: self.label(),
            sizing: self.sizing.word(),
            button: self.sizing.button(),
        }
        .render()
    }
}

#[derive(askama::Template)]
#[template(path = "title-bar.html")]
struct Page {
    dot: &'static str,
    word: &'static str,
    label: String,
    sizing: &'static str,
    button: &'static str,
}

/// A webview's place in the window, in physical pixels from the top left
/// of the window's client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// Pixels from the top.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Splits a client area of `width` by `height` physical pixels at `scale`
/// (physical pixels per logical one) into the bar on top and the pages
/// below. The bar keeps its height, so a tiny window shows only the bar; an
/// unusable scale counts as 1.
#[must_use]
pub fn split(width: u32, height: u32, scale: f64) -> (Place, Place) {
    let scale = if scale.is_finite() && scale > 0.0 && scale <= 16.0 {
        scale
    } else {
        1.0
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "32 times a scale bounded to 0..=16 is a small positive number"
    )]
    let bar = (HEIGHT * scale).round() as u32;
    let bar = bar.min(height);
    (
        Place {
            y: 0,
            width,
            height: bar,
        },
        Place {
            y: bar,
            width,
            height: height.saturating_sub(bar),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::status::Health;

    // ADR 0019 (title bar): the middle button says what it will do, as
    // Windows names it: Maximize for a normal window, Restore for a
    // maximized one.
    #[test]
    fn the_middle_button_maximizes_or_restores() {
        assert_eq!(Sizing::of(false).button(), "Maximize");
        assert_eq!(Sizing::of(true).button(), "Restore");
        assert_eq!(Sizing::of(true).word(), "maximized");
        let html = View {
            shown: Shown::Off,
            sizing: Sizing::Maximized,
        }
        .render()
        .expect("the bar renders");
        assert!(html.contains("data-sizing=\"maximized\""), "{html}");
        assert!(
            html.contains(
                "data-window=\"toggle-maximize\" aria-label=\"Restore\" title=\"Restore\""
            )
        );
    }

    // ADR 0019 (title bar): the dot has the tray's four states, never colour
    // alone: the word is its title and accessible name.
    #[test]
    fn the_dot_shows_the_trays_four_states_with_their_words() {
        let cases = [
            (
                Shown::Running(Health::Good),
                "live-dot live-dot--still",
                "All good",
            ),
            (
                Shown::Running(Health::Attention),
                "live-dot live-dot--still live-dot--warn",
                "Something needs you",
            ),
            (
                Shown::Running(Health::Problem),
                "live-dot live-dot--still live-dot--bad",
                "A problem needs you",
            ),
            (
                Shown::Running(Health::Unknown),
                "live-dot live-dot--still live-dot--bad",
                "The engine could not read its state",
            ),
            (
                Shown::Off,
                "live-dot live-dot--still live-dot--off",
                "The engine isn’t answering",
            ),
            (
                Shown::Checking,
                "live-dot live-dot--still live-dot--off",
                "Checking the engine…",
            ),
        ];
        for (shown, dot, word) in cases {
            let view = View {
                shown,
                sizing: Sizing::Normal,
            };
            assert_eq!((view.dot(), view.word()), (dot, word), "{shown:?}");
            let html = view.render().expect("the bar renders");
            assert!(
                html.contains(&format!(
                    "aria-label=\"Engine: {word}\" title=\"{word}\" data-bar-state><span class=\"{dot}\">"
                )),
                "{html}"
            );
        }
    }

    // ADR 0019 (title bar): the script hands the bar only these words as
    // JSON; the bar's page holds no other way in.
    #[test]
    fn the_script_passes_the_view_as_json() {
        let script = View {
            shown: Shown::Running(Health::Attention),
            sizing: Sizing::Maximized,
        }
        .script();
        let json = script
            .strip_prefix("window.analyteTitleBar && window.analyteTitleBar(")
            .and_then(|rest| rest.strip_suffix(");"))
            .expect("one call");
        let view: serde_json::Value = serde_json::from_str(json).expect("JSON");
        assert_eq!(view["word"], "Something needs you");
        assert_eq!(view["label"], "Engine: Something needs you");
        assert_eq!(view["dot"], "live-dot live-dot--still live-dot--warn");
        assert_eq!(view["sizing"], "maximized");
        assert_eq!(view["button"], "Restore");
    }

    // ADR 0019 (title bar): the page has the mark from the hand-drawn 16 and
    // 20 px files, three named buttons, and loads only the app's own files.
    #[test]
    fn the_page_has_the_mark_and_three_named_buttons() {
        let html = View::default().render().expect("the bar renders");
        assert!(html.contains("src=\"/static/icons/analyte-app-icon-16.svg\""));
        assert!(html.contains("/static/icons/analyte-app-icon-20.svg 1.25x"));
        for (action, name) in [
            ("minimize", "Minimize"),
            ("toggle-maximize", "Maximize"),
            ("close", "Close"),
        ] {
            assert!(
                html.contains(&format!(
                    "<button type=\"button\" class=\"title-bar__button{}\" data-window=\"{action}\" aria-label=\"{name}\" title=\"{name}\">",
                    if action == "close" { " title-bar__button--close" } else { "" }
                )),
                "{action}: {html}"
            );
        }
        assert!(html.contains("<script src=\"/static/title-bar.js\" defer></script>"));
        for absent in ["http://", "https://", "style=", "onclick"] {
            assert!(!html.contains(absent), "{absent}");
        }
    }

    // ADR 0019 (title bar): the bar is 32 logical pixels at every scale and
    // the pages take the rest; a window shorter than the bar shows only it.
    #[test]
    fn the_bar_keeps_its_height_and_the_pages_take_the_rest() {
        assert_eq!(
            split(1280, 800, 1.0),
            (
                Place {
                    y: 0,
                    width: 1280,
                    height: 32
                },
                Place {
                    y: 32,
                    width: 1280,
                    height: 768
                }
            )
        );
        assert_eq!(
            split(1600, 1000, 1.25).1,
            Place {
                y: 40,
                width: 1600,
                height: 960
            }
        );
        assert_eq!(split(2560, 1600, 2.0).0.height, 64);
        assert_eq!(split(1500, 900, 1.5).1.y, 48);
        assert_eq!(split(300, 20, 1.0).0.height, 20);
        assert_eq!(split(300, 20, 1.0).1.height, 0);
        for unusable in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e9] {
            assert_eq!(split(100, 100, unusable).0.height, 32, "{unusable}");
        }
    }
}
