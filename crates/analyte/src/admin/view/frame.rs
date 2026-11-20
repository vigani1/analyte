//! Holds what a page's frame shows: its title, where its files load from and, once signed in, the sidebar.
//!
//! The sidebar says only what the page observed when it was made: the engine
//! answered ("Engine running") and when ("Page loaded 10:45"). There is no
//! live channel yet, so nothing pulses.
use super::{Tab, assets::Root, format, icon::Icon};
use crate::admin::{server::Context, sessions::Session};
use jiff::Timestamp;

/// The page frame: the `<title>`, the static file root and the sidebar.
pub(in crate::admin) struct Frame {
    pub(in crate::admin) title: String,
    /// This build's static file root, such as `/static/0123456789abcdef/`.
    pub(in crate::admin) assets: String,
    pub(in crate::admin) sidebar: Sidebar,
}

/// The plain frame of sign-in and the refusals: the `<title>` and the static
/// file root, and no sidebar, so a page that may be shown before sign-in
/// names no machine, lab app or count.
pub(in crate::admin) struct Plain {
    pub(in crate::admin) title: String,
    /// This build's static file root, such as `/static/0123456789abcdef/`.
    pub(in crate::admin) assets: String,
}

impl Plain {
    pub(in crate::admin) fn new(assets: &Root, title: &str) -> Self {
        Self {
            title: title.to_owned(),
            assets: assets.as_str().to_owned(),
        }
    }
}

/// The sidebar: engine box, main links, the "More" group and the sign-out foot.
pub(in crate::admin) struct Sidebar {
    /// When the page was made, such as "Page loaded 10:45".
    pub(in crate::admin) loaded: String,
    pub(in crate::admin) main: Vec<NavLink>,
    pub(in crate::admin) more: Vec<MoreLink>,
    /// The current page sits under "More", so the group starts open.
    pub(in crate::admin) more_open: bool,
    /// The session's form token for Sign out; none when the request used the
    /// admin key itself, which has no session to end.
    pub(in crate::admin) csrf: Option<String>,
}

/// One main link with its icon; `count` is shown beside it when some.
pub(in crate::admin) struct NavLink {
    pub(in crate::admin) tab: Tab,
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) current: bool,
    pub(in crate::admin) count: Option<u64>,
}

/// One link in the "More" group.
pub(in crate::admin) struct MoreLink {
    pub(in crate::admin) tab: Tab,
    pub(in crate::admin) current: bool,
}

/// The main links in order, as the Sidebar mockup lists them.
const MAIN: [(Tab, Icon); 6] = [
    (Tab::Overview, Icon::Overview),
    (Tab::Activity, Icon::Activity),
    (Tab::NeedsReview, Icon::Review),
    (Tab::Machines, Icon::Machine),
    (Tab::LabApps, Icon::LabApp),
    (Tab::Setup, Icon::Setup),
];

/// The pages under "More", as the Sidebar mockup lists them.
const MORE: [Tab; 5] = [
    Tab::MachineOrders,
    Tab::CancelledByMachine,
    Tab::Deliveries,
    Tab::DataKeeping,
    Tab::Settings,
];

impl Frame {
    /// The frame of page `current`. `review` is the open Needs review count,
    /// none when it could not be read.
    pub(in crate::admin) fn new(
        state: &Context,
        current: Tab,
        title: impl Into<String>,
        review: Option<u64>,
        session: Option<&Session>,
        now: Timestamp,
    ) -> Self {
        Self {
            title: title.into(),
            assets: state.assets.as_str().to_owned(),
            sidebar: Sidebar {
                loaded: format::page_loaded(now),
                main: MAIN
                    .into_iter()
                    .map(|(tab, icon)| NavLink {
                        tab,
                        icon,
                        current: tab == current,
                        count: review.filter(|&count| tab == Tab::NeedsReview && count > 0),
                    })
                    .collect(),
                more: MORE
                    .into_iter()
                    .map(|tab| MoreLink {
                        tab,
                        current: tab == current,
                    })
                    .collect(),
                more_open: MORE.contains(&current),
                csrf: session
                    .filter(|session| session.authenticated)
                    .map(|session| session.csrf.form_value().to_owned()),
            },
        }
    }
}
