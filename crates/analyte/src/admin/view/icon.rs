//! Holds each Lucide icon the restyled pages draw (ISC licence), its SVG path written once.
//!
//! The `icon` template macro draws `<svg class="icon"><path d="…"/></svg>` from
//! [`Icon::path`]. Paths use only letters, digits, spaces, `.`, `-` and `,`, so
//! the template's escaping leaves them unchanged; a test holds that.
use crate::admin::server::Connection;

/// One icon a page or the sidebar draws; only icons in use are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Icon {
    /// The Overview tab: four tiles.
    Overview,
    /// The Needs review tab: an inbox tray.
    Review,
    /// The Machines tab and machine nodes: an analyzer.
    Machine,
    /// The Lab apps tab and lab app nodes: two stacked servers.
    LabApp,
    /// The Setup tab: sliders.
    Setup,
    /// The More group: three dots.
    More,
    /// Opens a group downwards.
    ChevronDown,
    /// Opens a disclosure to the right.
    ChevronRight,
    /// Good: a ticked circle.
    Ok,
    /// Needs a look: a warning triangle.
    Warning,
    /// A problem: a crossed octagon.
    Problem,
    /// Waiting or off: an hourglass.
    Waiting,
    /// A serial cable: a plug.
    Serial,
    /// A USB adapter.
    Usb,
    /// A network link.
    Network,
    /// A watched folder.
    Folder,
    /// A missing key, where a lab app's wire breaks.
    Key,
    /// A broken link, where a machine's wire breaks.
    Cross,
    /// An unknown outcome: a question mark in a circle.
    Unknown,
    /// Cancelled: a crossed circle.
    Cancelled,
    /// Nothing left to do: a tick, as on an empty Needs review list.
    Check,
    /// Adds something, such as a machine: a plus.
    Plus,
    /// Something worth knowing: an i in a circle.
    Info,
    /// Kept locked, such as a lab app's keys: a padlock.
    Lock,
    /// Taken or off, such as a port another machine uses: a slashed circle.
    Off,
    /// Being sent, such as an order on its way to a machine: a paper plane.
    Send,
}

impl Icon {
    /// The icon's SVG path on Lucide's 24-unit grid.
    pub(in crate::admin) const fn path(self) -> &'static str {
        match self {
            Self::Overview => concat!(
                "M4 3h5a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z ",
                "M15 3h5a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z ",
                "M15 12h5a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1Z ",
                "M4 16h5a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1Z",
            ),
            Self::Review => concat!(
                "M22 12h-6l-2 3h-4l-2-3H2 M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 ",
                "2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11Z",
            ),
            Self::Machine => concat!(
                "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z ",
                "M7 8h7 M7 16h4 M15 16a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0",
            ),
            Self::LabApp => concat!(
                "M4 2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2Z ",
                "M4 14h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2Z ",
                "M6 6h.01 M6 18h.01",
            ),
            Self::Setup => concat!(
                "M4 21v-7 M4 10V3 M12 21v-9 M12 8V3 M20 21v-5 M20 12V3 M1 14h6 M9 8h6 ",
                "M17 16h6",
            ),
            Self::More => concat!(
                "M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0 M18 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0 ",
                "M4 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0",
            ),
            Self::ChevronDown => "m6 9 6 6 6-6",
            Self::ChevronRight => "m9 18 6-6-6-6",
            Self::Ok => "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0 M8.5 12.5l2.5 2.5 4.5-5",
            Self::Warning => concat!(
                "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 ",
                "1.73-3Z M12 9v4 M12 17h.01",
            ),
            Self::Problem => concat!(
                "M2.586 16.726A2 2 0 0 1 2 15.312V8.688a2 2 0 0 1 .586-1.414l4.688-4.688",
                "A2 2 0 0 1 8.688 2h6.624a2 2 0 0 1 1.414.586l4.688 4.688A2 2 0 0 1 22 ",
                "8.688v6.624a2 2 0 0 1-.586 1.414l-4.688 4.688a2 2 0 0 1-1.414.586H8.688",
                "a2 2 0 0 1-1.414-.586z M15 9l-6 6 M9 9l6 6",
            ),
            Self::Waiting => concat!(
                "M5 22h14 M5 2h14 M17 22v-4.172a2 2 0 0 0-.586-1.414L12 12l-4.414 ",
                "4.414A2 2 0 0 0 7 17.828V22 M7 2v4.172a2 2 0 0 0 .586 1.414L12 12l",
                "4.414-4.414A2 2 0 0 0 17 6.172V2",
            ),
            Self::Serial => "M12 22v-5 M9 8V2 M15 8V2 M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z",
            Self::Usb => concat!(
                "M9 7a1 1 0 1 0 2 0a1 1 0 1 0-2 0 M3 20a1 1 0 1 0 2 0a1 1 0 1 0-2 0 ",
                "M4.7 19.3 19 5 M21 3l-3 1 2 2Z M9.26 7.68 5 12l2 5 M10 14l5 2 3.5-3.5 ",
                "M18 12l1-1 1 1-1 1Z",
            ),
            Self::Network => concat!(
                "M17 16h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1Z ",
                "M3 16h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1Z ",
                "M10 2h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1Z ",
                "M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3 M12 12V8",
            ),
            Self::Folder => concat!(
                "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9",
                "A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
            ),
            Self::Key => concat!(
                "m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4 ",
                "M21 2l-9.6 9.6 M2 15.5a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0-11 0",
            ),
            Self::Cross => "M17 7 7 17 M7 7l10 10",
            Self::Unknown => concat!(
                "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0 M9.09 9a3 3 0 0 1 5.83 1c0 ",
                "2-3 3-3 3 M12 17h.01",
            ),
            Self::Cancelled => "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0 M15 9l-6 6 M9 9l6 6",
            Self::Check => "M20 6 9 17l-5-5",
            Self::Plus => "M12 5v14 M5 12h14",
            Self::Info => "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0 M12 16v-4 M12 8h.01",
            Self::Lock => concat!(
                "M5 11h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2Z ",
                "M7 11V7a5 5 0 0 1 10 0v4",
            ),
            Self::Off => "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0 M4.9 4.9l14.2 14.2",
            Self::Send => concat!(
                "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 ",
                "6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z M21.854 2.147l-10.94 ",
                "10.939",
            ),
        }
    }

    /// How a machine is attached, as an icon.
    pub(in crate::admin) const fn connection(connection: &Connection) -> Self {
        match connection {
            Connection::Serial(_) => Self::Serial,
            Connection::UsbAdapter(_) => Self::Usb,
            Connection::Listening(_) | Connection::Dialing => Self::Network,
            Connection::Folder(_) => Self::Folder,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; 26] = [
        Icon::Overview,
        Icon::Review,
        Icon::Machine,
        Icon::LabApp,
        Icon::Setup,
        Icon::More,
        Icon::ChevronDown,
        Icon::ChevronRight,
        Icon::Ok,
        Icon::Warning,
        Icon::Problem,
        Icon::Waiting,
        Icon::Serial,
        Icon::Usb,
        Icon::Network,
        Icon::Folder,
        Icon::Key,
        Icon::Cross,
        Icon::Unknown,
        Icon::Cancelled,
        Icon::Check,
        Icon::Plus,
        Icon::Info,
        Icon::Lock,
        Icon::Off,
        Icon::Send,
    ];

    // ADR 0018: icon paths go through the template's escaping, which must leave
    // them as they are; letters, digits, spaces, dots, minus signs and commas
    // are never escaped, so no path needs `|safe`.
    #[test]
    fn icon_paths_hold_only_characters_escaping_keeps() {
        for icon in ALL {
            let path = icon.path();
            assert!(!path.is_empty(), "{icon:?}");
            assert!(
                path.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | ',')),
                "{icon:?}: {path}"
            );
            assert!(
                !path.contains("  ") && !path.ends_with(' '),
                "{icon:?}: {path}"
            );
        }
    }
}
