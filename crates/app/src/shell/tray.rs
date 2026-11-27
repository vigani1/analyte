//! The tray icon: the Aperture mark with a badge for the engine's state, its tooltip, and the menu (Open Analyte, the state line, Start at sign-in, Quit app).
//!
//! "Quit app" ends only this app; the engine keeps running as a service.
use super::{Command, Shared, autostart};
use crate::watch::{Shown, Tray};
use analyte_i18n::{message as tr_args, text as tr};
use std::sync::Arc;
use tauri::{
    AppHandle, Manager as _, Wry,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const ID: &str = "analyte";

/// The menu items that change.
struct Parts {
    state: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

/// One state's icon at each size: 16 and 20 px drawn by hand, 24 and 32 px
/// from the large drawing (`scripts/make-app-icon.py`).
macro_rules! icons {
    ($name:literal) => {
        [
            (
                16,
                include_bytes!(concat!("../../icons/tray-", $name, "-16.png")).as_slice(),
            ),
            (
                20,
                include_bytes!(concat!("../../icons/tray-", $name, "-20.png")).as_slice(),
            ),
            (
                24,
                include_bytes!(concat!("../../icons/tray-", $name, "-24.png")).as_slice(),
            ),
            (
                32,
                include_bytes!(concat!("../../icons/tray-", $name, "-32.png")).as_slice(),
            ),
        ]
    };
}

const GOOD: [(u32, &[u8]); 4] = icons!("good");
const ATTENTION: [(u32, &[u8]); 4] = icons!("attention");
const PROBLEM: [(u32, &[u8]); 4] = icons!("problem");
const OFF: [(u32, &[u8]); 4] = icons!("off");

/// The tray icon's size in pixels for the main screen's scale: 16 at 100 %,
/// 20 at 125 %, 24 at 150 %, 32 from 200 %.
fn size(app: &AppHandle) -> u32 {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    if scale < 1.125 {
        16
    } else if scale < 1.375 {
        20
    } else if scale < 1.75 {
        24
    } else {
        32
    }
}

fn icon(app: &AppHandle, tray: Tray) -> tauri::Result<Image<'static>> {
    let set = match tray {
        Tray::Good => &GOOD,
        Tray::Attention => &ATTENTION,
        Tray::Problem => &PROBLEM,
        Tray::Off => &OFF,
    };
    let wanted = size(app);
    let bytes = set
        .iter()
        .find(|(side, _)| *side == wanted)
        .or_else(|| set.first())
        .map_or(&[][..], |(_, bytes)| bytes);
    Image::from_bytes(bytes)
}

fn tooltip(shown: Shown) -> String {
    tr_args(
        "desktop-analyte",
        [("words", (shown.words()).to_string().into())],
    )
}

/// Makes the tray icon and its menu.
pub(super) fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", tr("desktop-open-analyte"), true, None::<&str>)?;
    let state = MenuItem::with_id(app, "state", Shown::Checking.words(), false, None::<&str>)?;
    let on = autostart::is_on().unwrap_or_else(|error| {
        tracing::warn!(%error, "whether the app starts at sign-in could not be read");
        false
    });
    let starts = CheckMenuItem::with_id(
        app,
        "autostart",
        tr("desktop-start-at-sign-in"),
        true,
        on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", tr("desktop-quit-app"), true, None::<&str>)?;
    let note = MenuItem::with_id(
        app,
        "note",
        tr("desktop-the-engine-keeps-running"),
        false,
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &state,
            &PredefinedMenuItem::separator(app)?,
            &starts,
            &PredefinedMenuItem::separator(app)?,
            &quit,
            &note,
        ],
    )?;
    TrayIconBuilder::with_id(ID)
        .icon(icon(app, Tray::Off)?)
        .tooltip(tooltip(Shown::Checking))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => app.state::<Arc<Shared>>().send(Command::Open),
            "autostart" => switch_autostart(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                tray.app_handle().state::<Arc<Shared>>().send(Command::Open);
            }
        })
        .build(app)?;
    app.manage(Parts {
        state,
        autostart: starts,
    });
    Ok(())
}

/// Shows `shown` in the icon, the tooltip and the menu's state line.
pub(super) fn show(app: &AppHandle, shown: Shown) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    let results = [
        icon(app, shown.tray()).and_then(|image| tray.set_icon(Some(image))),
        tray.set_tooltip(Some(tooltip(shown))),
        app.state::<Parts>().state.set_text(shown.words()),
    ];
    for result in results {
        if let Err(error) = result {
            tracing::warn!(%error, "the tray could not show the state");
        }
    }
}

/// Turns starting at sign-in on or off as the menu's tick now says; on
/// failure the tick goes back to what Windows holds.
fn switch_autostart(app: &AppHandle) {
    let item = &app.state::<Parts>().autostart;
    let wanted = item.is_checked().unwrap_or(false);
    if let Err(error) = autostart::set(wanted) {
        tracing::warn!(%error, wanted, "starting at sign-in could not be changed");
        let held = autostart::is_on().unwrap_or(!wanted);
        if let Err(error) = item.set_checked(held) {
            tracing::warn!(%error, "the menu tick could not be reset");
        }
    }
}
