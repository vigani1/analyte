//! The one window: the engine's admin pages while the engine answers, the app's Engine off screen while it does not, and nothing else.
//!
//! Closing it destroys its webview, so its memory is freed; the tray makes
//! a new one. Links to other sites open in the default browser, where they
//! carry no admin session.
use super::Shared;
use crate::engine::{Engine, Origin};
use crate::local::ENGINE_OFF;
use crate::navigation::{Decision, decide};
use crate::watch::Shown;
use tauri::{
    AppHandle, Manager as _, WebviewUrl, WebviewWindowBuilder, webview::NewWindowResponse,
};

/// The window's label.
const LABEL: &str = "main";

/// The Engine off screen's address.
fn engine_off() -> String {
    format!("{}{ENGINE_OFF}", Origin::Local.text())
}

/// Where the window should be for `shown`.
fn target(shared: &Shared, shown: Shown) -> String {
    match (&shared.engine, shown) {
        (Ok(engine), Shown::Running(_)) => engine.home(),
        (Ok(_) | Err(_), _) => engine_off(),
    }
}

/// The origin the window may show besides the app's own: the engine's, or
/// only the app's own when the settings named no engine.
fn engine_origin(shared: &Shared) -> Origin {
    shared.engine.as_ref().map_or(Origin::Local, Engine::origin)
}

/// Whether the window shows `url`; a web link opens in the browser instead.
fn allowed(url: &url::Url, origin: Origin) -> bool {
    match decide(url, origin) {
        Decision::Show => true,
        Decision::OpenOutside => {
            if let Err(error) = tauri_plugin_opener::open_url(url.as_str(), None::<&str>) {
                tracing::warn!(%error, "a link could not be opened in the browser");
            }
            false
        }
        Decision::Refuse => {
            tracing::warn!(scheme = url.scheme(), "the window refused an address");
            false
        }
    }
}

/// Shows the window, making it if there is none.
pub(super) fn open(app: &AppHandle, shared: &Shared, shown: Shown) {
    if let Some(window) = app.get_webview_window(LABEL) {
        for result in [window.unminimize(), window.show(), window.set_focus()] {
            if let Err(error) = result {
                tracing::warn!(%error, "the window could not be brought forward");
            }
        }
        return;
    }
    let address = target(shared, shown);
    let url = match url::Url::parse(&address) {
        Ok(url) => url,
        Err(error) => {
            tracing::error!(%error, "the window's address is not valid");
            return;
        }
    };
    let origin = engine_origin(shared);
    let opener = app.clone();
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
        .title("Analyte")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .on_navigation(move |url| allowed(url, origin))
        .on_new_window(move |url, _| {
            // A link meant for a new window: an engine page opens here, the web in the browser.
            if allowed(&url, origin)
                && let Some(window) = opener.get_webview_window(LABEL)
                && let Err(error) = window.navigate(url)
            {
                tracing::warn!(%error, "the window could not follow a link");
            }
            NewWindowResponse::Deny
        })
        .build();
    if let Err(error) = built {
        // Usually the WebView2 Runtime is missing; the pages still open in the browser.
        tracing::error!(%error, "the window could not be made; opening the pages in the browser");
        if let Err(error) = tauri_plugin_opener::open_url(&address, None::<&str>) {
            tracing::error!(%error, "the pages could not be opened in the browser either");
        }
    }
}

/// Moves the window to where `shown` wants it: the pages once the engine
/// answers, the Engine off screen once it does not, and a fresh Engine off
/// screen when what it shows `changed`.
pub(super) fn follow(app: &AppHandle, shared: &Shared, shown: Shown, changed: bool) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let current = match window.url() {
        Ok(current) => current,
        Err(error) => {
            tracing::warn!(%error, "the window's address could not be read");
            return;
        }
    };
    let on_local = Origin::Local.holds(&current);
    let go = match shown {
        Shown::Running(_) => on_local,
        Shown::Checking | Shown::Off => !on_local || changed,
    };
    if !go {
        return;
    }
    match url::Url::parse(&target(shared, shown)) {
        Ok(url) => {
            if let Err(error) = window.navigate(url) {
                tracing::warn!(%error, "the window could not change page");
            }
        }
        Err(error) => tracing::error!(%error, "the window's address is not valid"),
    }
}
