//! The app's own title bar: the Aperture mark and "Analyte", a dot with the engine's state and its word, and the minimize, maximize or restore, and close buttons; where it and its maximize button sit in the window, and what the shell tells it.
//!
//! The bar is the app's own page (`http://analyte.localhost/title-bar`) in a
//! webview of its own above the engine's pages (ADR 0019). It is the only
//! webview allowed any Tauri command, and only the window commands it needs;
//! the engine's pages stay without any. The bar never asks the app anything:
//! the shell pushes what it shows with [`View::script`].
//!
//! On Windows a small native window lies over the maximize button, so that
//! Windows takes it for the frame's own button and shows snap layouts on
//! hover (ADR 0022). The mouse then reaches that window, not the page, so the
//! shell follows it with [`Pointer`] and gives the button its hover and
//! pressed looks.
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

/// A caption button's width in logical pixels, as `--caption-button` in
/// `app.css`.
pub const BUTTON: f64 = 46.0;

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

    /// Whether the window menu offers `item` for a window of this sizing, as
    /// for a framed window: a normal one cannot be restored, a maximized one
    /// cannot be moved, sized or maximized.
    #[must_use]
    pub const fn offers(self, item: MenuItem) -> bool {
        match (self, item) {
            (Self::Normal, MenuItem::Restore)
            | (Self::Maximized, MenuItem::Move | MenuItem::Size | MenuItem::Maximize) => false,
            (
                Self::Normal,
                MenuItem::Move
                | MenuItem::Size
                | MenuItem::Minimize
                | MenuItem::Maximize
                | MenuItem::Close,
            )
            | (Self::Maximized, MenuItem::Restore | MenuItem::Minimize | MenuItem::Close) => true,
        }
    }
}

/// A command in the window menu, which Windows shows on a right-click of the
/// bar or on Alt+Space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// Restore.
    Restore,
    /// Move.
    Move,
    /// Size.
    Size,
    /// Minimize.
    Minimize,
    /// Maximize.
    Maximize,
    /// Close.
    Close,
}

impl MenuItem {
    /// Every item, in the menu's order.
    pub const ALL: [Self; 6] = [
        Self::Restore,
        Self::Move,
        Self::Size,
        Self::Minimize,
        Self::Maximize,
        Self::Close,
    ];
}

/// Where the mouse is on the maximize button, which Windows reports to the
/// native window over it rather than to the page: the button's look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pointer {
    /// Elsewhere: the button's resting look.
    #[default]
    Away,
    /// Over the button: its hover look.
    Over,
    /// Pressed on the button and not yet released: its pressed look.
    Pressed,
}

/// What Windows reported about the mouse over the maximize button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    /// The mouse moved over the button.
    Move,
    /// The left button went down on it.
    Down,
    /// The left button came up on it.
    Up,
    /// The mouse left it.
    Leave,
}

impl Pointer {
    /// The look after `mouse`. A press stays pressed while the mouse moves
    /// over the button and ends when it leaves, as with Windows' own buttons.
    #[must_use]
    pub const fn after(self, mouse: Mouse) -> Self {
        match (self, mouse) {
            (Self::Pressed, Mouse::Move) | (_, Mouse::Down) => Self::Pressed,
            (Self::Away | Self::Over, Mouse::Move) | (_, Mouse::Up) => Self::Over,
            (_, Mouse::Leave) => Self::Away,
        }
    }

    /// Whether `mouse` completes a click: released on the button it was
    /// pressed on. Leaving the button ends the press, so a release after
    /// coming back does nothing.
    #[must_use]
    pub const fn clicks(self, mouse: Mouse) -> bool {
        matches!((self, mouse), (Self::Pressed, Mouse::Up))
    }

    /// The button's `data-pointer` value, which picks its look.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Away => "away",
            Self::Over => "over",
            Self::Pressed => "pressed",
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
    /// Where the mouse is on the maximize button.
    pub pointer: Pointer,
}

impl Default for View {
    fn default() -> Self {
        Self {
            shown: Shown::Checking,
            sizing: Sizing::Normal,
            pointer: Pointer::Away,
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
            "pointer": self.pointer.word(),
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
            pointer: self.pointer.word(),
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
    pointer: &'static str,
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

/// An area of the window in physical pixels from the top left of its client
/// area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Pixels from the left.
    pub x: u32,
    /// Pixels from the top.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Where the maximize button sits in a client area `width` physical pixels
/// wide whose bar is at `bar`, at `scale`: the middle of the three buttons at
/// the bar's right end, each [`BUTTON`] logical pixels wide, its edges
/// rounded to whole pixels. A window narrower than two buttons, or a bar of
/// no height, has none; an unusable scale counts as 1.
#[must_use]
pub fn maximize_button(width: u32, bar: Place, scale: f64) -> Option<Area> {
    let scale = usable(scale);
    let from_right = |buttons: f64| -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "two 46 px buttons times a scale bounded to 0..=16 is a small positive number"
        )]
        let pixels = (BUTTON * buttons * scale).round() as u32;
        pixels
    };
    let left = width.checked_sub(from_right(2.0))?;
    let right = width.checked_sub(from_right(1.0))?;
    (bar.height > 0).then_some(Area {
        x: left,
        y: bar.y,
        width: right.saturating_sub(left),
        height: bar.height,
    })
}

/// `scale` when it is a usable display scale (above 0, at most 16), else 1.
fn usable(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 && scale <= 16.0 {
        scale
    } else {
        1.0
    }
}

/// Splits a client area of `width` by `height` physical pixels at `scale`
/// (physical pixels per logical one) into the bar on top and the pages
/// below. The bar keeps its height, so a tiny window shows only the bar; an
/// unusable scale counts as 1.
#[must_use]
pub fn split(width: u32, height: u32, scale: f64) -> (Place, Place) {
    let scale = usable(scale);
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
            pointer: Pointer::Away,
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

    // ADR 0022 (window menu): the right-click menu offers what a framed
    // window's does: Restore only when maximized; Move, Size and Maximize
    // only when not; Minimize and Close always.
    #[test]
    fn the_window_menu_offers_what_the_sizing_allows() {
        let offered = |sizing: Sizing| {
            MenuItem::ALL
                .into_iter()
                .filter(|item| sizing.offers(*item))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            offered(Sizing::Normal),
            [
                MenuItem::Move,
                MenuItem::Size,
                MenuItem::Minimize,
                MenuItem::Maximize,
                MenuItem::Close
            ]
        );
        assert_eq!(
            offered(Sizing::Maximized),
            [MenuItem::Restore, MenuItem::Minimize, MenuItem::Close]
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
                pointer: Pointer::Away,
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
            pointer: Pointer::Pressed,
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
        assert_eq!(view["pointer"], "pressed");
    }

    // ADR 0019 (title bar): the page has the mark from the hand-drawn 16 and
    // 20 px files, three named buttons, and loads only the app's own files.
    #[test]
    fn the_page_has_the_mark_and_three_named_buttons() {
        let html = View::default().render().expect("the bar renders");
        assert!(html.contains("src=\"/static/icons/analyte-app-icon-16.svg\""));
        assert!(html.contains("/static/icons/analyte-app-icon-20.svg 1.25x"));
        for (action, name, more) in [
            ("minimize", "Minimize", ""),
            ("toggle-maximize", "Maximize", " data-pointer=\"away\""),
            ("close", "Close", ""),
        ] {
            assert!(
                html.contains(&format!(
                    "<button type=\"button\" class=\"title-bar__button{}\" data-window=\"{action}\" aria-label=\"{name}\" title=\"{name}\"{more}>",
                    if action == "close" { " title-bar__button--close" } else { "" }
                )),
                "{action}: {html}"
            );
        }
        // ADR 0022: the bar is a Windows caption through app-region, so
        // Tauri's drag attribute and its two permissions are gone.
        assert!(!html.contains("data-tauri-drag-region"));
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

    // ADR 0022: the native window for snap layouts covers exactly the
    // maximize button, the middle of three 46 px buttons at the bar's right
    // end, at every scale; its edges round as the page's do.
    #[test]
    fn the_maximize_area_is_the_middle_button() {
        let at =
            |width, height, scale| maximize_button(width, split(width, height, scale).0, scale);
        assert_eq!(
            at(1280, 800, 1.0),
            Some(Area {
                x: 1188,
                y: 0,
                width: 46,
                height: 32
            })
        );
        assert_eq!(
            at(2560, 1600, 2.0),
            Some(Area {
                x: 2376,
                y: 0,
                width: 92,
                height: 64
            })
        );
        // 46 × 1.25 = 57.5 and 92 × 1.25 = 115: the edges are whole pixels.
        assert_eq!(
            at(1600, 1000, 1.25),
            Some(Area {
                x: 1485,
                y: 0,
                width: 57,
                height: 40
            })
        );
        assert_eq!(
            at(1500, 900, 1.5).map(|area| (area.x, area.width)),
            Some((1362, 69))
        );
        for unusable in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e9] {
            assert_eq!(
                at(1280, 800, unusable).map(|area| area.x),
                Some(1188),
                "{unusable}"
            );
        }
    }

    // ADR 0022: a window too narrow for the buttons, or without a bar, has
    // no maximize area, never one off its edge.
    #[test]
    fn a_window_too_small_has_no_maximize_area() {
        assert_eq!(at_width(91), None);
        assert_eq!(at_width(92).map(|area| (area.x, area.width)), Some((0, 46)));
        let no_bar = Place {
            y: 0,
            width: 1280,
            height: 0,
        };
        assert_eq!(maximize_button(1280, no_bar, 1.0), None);
    }

    fn at_width(width: u32) -> Option<Area> {
        maximize_button(width, split(width, 800, 1.0).0, 1.0)
    }

    // ADR 0022 (Windows caption buttons): hovering shows the hover look,
    // pressing the pressed look, and only a release on the button it was
    // pressed on clicks; leaving ends both the hover and the press.
    #[test]
    fn the_maximize_button_clicks_only_on_a_release_over_it() {
        use Mouse::{Down, Leave, Move, Up};
        let mut pointer = Pointer::default();
        let mut clicks = 0;
        let mut looks = Vec::new();
        for mouse in [Move, Down, Move, Up, Move, Down, Leave, Move, Up, Leave] {
            clicks += u8::from(pointer.clicks(mouse));
            pointer = pointer.after(mouse);
            looks.push(pointer.word());
        }
        assert_eq!(clicks, 1);
        assert_eq!(
            looks,
            [
                "over", "pressed", "pressed", "over", "over", "pressed", "away", "over", "over",
                "away"
            ]
        );
        // A release that never had a press, and a second release, do nothing.
        assert!(!Pointer::Over.clicks(Up));
        assert!(!Pointer::Away.clicks(Up));
        assert!(Pointer::Pressed.clicks(Up));
        assert_eq!(Pointer::Away.after(Down), Pointer::Pressed);
    }
}
