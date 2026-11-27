//! The one window: the app's own title bar on top, and below it the engine's admin pages while the engine answers, the app's Engine off screen while it does not, and nothing else.
//!
//! The window has no Windows frame; it is two webviews (ADR 0019). The bar
//! is the app's own page, the only webview the `title-bar` capability lets
//! run window commands; the pages' webview gets no Tauri command at all.
//! Windows still gives the frameless window its shadow, edge resizing,
//! snapping (by dragging, Win+arrow keys and Win+Z) and Alt+F4, and Tauri
//! adds a strip along the top edge for resizing above the bar. The bar's page
//! is a Windows caption through `WebView2`'s `app-region`, so Windows drags the
//! window, maximizes it on a double-click and shows the window menu on a
//! right-click; [`super::frame`] adds snap layouts on the maximize button and
//! the window menu on Alt+Space (ADR 0022).
//!
//! Closing the window destroys both webviews, so their memory is freed; the
//! tray makes a new window. Links to other sites open in the default browser,
//! where they carry no admin session; the pages run the app's one script so
//! such a link asks for a new window and nothing is sent from the window
//! (see [`crate::navigation::PAGES_SCRIPT`]). The pages' webview carries the
//! `AnalyteApp` user agent, and the window runs Windows sign-in itself
//! (ADR 0021 §6): it opens at `/login/windows?next=/`, and every
//! `/login/windows` without a code becomes a sign-in for the monitor.
use super::{Command, Shared, frame};
use crate::engine::{Engine, Origin};
use crate::local::ENGINE_OFF;
use crate::navigation::{Decision, PAGES_SCRIPT, decide};
use crate::sign_in;
use crate::title_bar::{self, Mouse, Place, Pointer, Sizing, View};
use crate::watch::Shown;
use analyte_i18n::text as tr;
use tauri::{
    AppHandle, Manager as _, PhysicalPosition, PhysicalSize, Rect, Runtime, WebviewUrl, Window,
    WindowEvent,
    webview::{NewWindowResponse, PageLoadEvent, Webview, WebviewBuilder},
};

/// The window's label.
pub(super) const LABEL: &str = "main";

/// The Engine off screen's address.
fn engine_off() -> String {
    format!("{}{ENGINE_OFF}", Origin::Local.text())
}

/// The title bar's address.
fn bar_address() -> String {
    format!("{}{}", Origin::Local.text(), title_bar::PAGE)
}

/// Where the pages should be for `shown`: on a running engine, Windows
/// sign-in to the Overview, so the window signs in at once.
fn target(shared: &Shared, shown: Shown) -> String {
    match (&shared.engine, shown) {
        (Ok(engine), Shown::Running(_)) => sign_in::start(engine),
        (Ok(_) | Err(_), _) => engine_off(),
    }
}

/// Where the Overview is for `shown`: the engine's first page on a running
/// engine, which signs the window in first when it has to (ADR 0021 §6),
/// else the Engine off screen.
fn overview(shared: &Shared, shown: Shown) -> String {
    match (&shared.engine, shown) {
        (Ok(engine), Shown::Running(_)) => format!("{}/", engine.origin().text()),
        (Ok(_) | Err(_), _) => engine_off(),
    }
}

/// The origin the pages may show besides the app's own: the engine's, or
/// only the app's own when the settings named no engine.
fn engine_origin(shared: &Shared) -> Origin {
    shared.engine.as_ref().map_or(Origin::Local, Engine::origin)
}

/// Whether the pages show `url`; a web link opens in the browser instead,
/// and Windows sign-in goes to the monitor while the pages stay put.
fn allowed<R: Runtime>(app: &AppHandle<R>, url: &url::Url, origin: Origin) -> bool {
    match decide(url, origin) {
        Decision::Show => true,
        Decision::WindowsSignIn { next, trigger } => {
            if let Some(shared) = app.try_state::<std::sync::Arc<Shared>>() {
                shared.send(Command::SignIn { next, trigger });
            }
            false
        }
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

/// The title bar's webview: its own page and nothing else, no new windows,
/// and brought up to date each time its page has loaded.
pub(crate) fn bar_webview<R: Runtime>(bar: url::Url) -> WebviewBuilder<R> {
    let only = bar.clone();
    WebviewBuilder::new(title_bar::LABEL, WebviewUrl::External(bar))
        .on_navigation(move |url| {
            let same = *url == only;
            if !same {
                tracing::warn!(scheme = url.scheme(), "the title bar refused an address");
            }
            same
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished
                && let Some(shared) = webview.try_state::<std::sync::Arc<Shared>>()
            {
                show(&webview, &shared.bar());
            }
        })
        .disable_drag_drop_handler()
}

/// The pages' webview at `url`, marked as the app for the engine's sign-in:
/// the engine's origin and the app's own pages show; web links open in the
/// browser, through a new-window request so the window sends nothing to
/// them; Windows sign-in runs in the app.
pub(crate) fn pages_webview<R: Runtime>(
    app: &AppHandle<R>,
    url: url::Url,
    origin: Origin,
) -> WebviewBuilder<R> {
    let opener = app.clone();
    let navigator = app.clone();
    WebviewBuilder::new(title_bar::PAGES, WebviewUrl::External(url))
        .user_agent(sign_in::USER_AGENT)
        .initialization_script(PAGES_SCRIPT)
        .on_navigation(move |url| allowed(&navigator, url, origin))
        .on_new_window(move |url, _| {
            // A link meant for a new window: an engine page opens here, the web in the browser.
            if allowed(&opener, &url, origin)
                && let Some(pages) = opener.get_webview(title_bar::PAGES)
                && let Err(error) = pages.navigate(url)
            {
                tracing::warn!(%error, "the window could not follow a link");
            }
            NewWindowResponse::Deny
        })
}

/// Shows the window, making it if there is none.
pub(super) fn open(app: &AppHandle, shared: &Shared, shown: Shown) {
    if let Some(window) = app.get_window(LABEL) {
        for result in [window.unminimize(), window.show(), window.set_focus()] {
            if let Err(error) = result {
                tracing::warn!(%error, "the window could not be brought forward");
            }
        }
        return;
    }
    let address = target(shared, shown);
    let (url, bar) = match (url::Url::parse(&address), url::Url::parse(&bar_address())) {
        (Ok(url), Ok(bar)) => (url, bar),
        (Err(error), _) | (_, Err(error)) => {
            tracing::error!(%error, "the window's address is not valid");
            return;
        }
    };
    // A new window starts at its normal size, with the mouse elsewhere.
    shared.update_bar(|view| {
        view.sizing = Sizing::Normal;
        view.pointer = Pointer::Away;
    });
    if let Err(error) = make(app, url, bar, engine_origin(shared)) {
        // Usually the WebView2 Runtime is missing; the pages still open in the browser.
        tracing::error!(%error, "the window could not be made; opening the pages in the browser");
        if let Some(window) = app.get_window(LABEL)
            && let Err(error) = window.destroy()
        {
            tracing::warn!(%error, "the half-made window could not be closed");
        }
        if let Err(error) = tauri_plugin_opener::open_url(&address, None::<&str>) {
            tracing::error!(%error, "the pages could not be opened in the browser either");
        }
    }
}

/// Shows the window on the Overview (ADR 0023): a new window starts there,
/// and one already open leaves the page it was on.
pub(super) fn open_overview(app: &AppHandle, shared: &Shared, shown: Shown) {
    let open = app.get_window(LABEL).is_some();
    tracing::info!(open, "opening the window on the overview");
    self::open(app, shared, shown);
    if !open {
        return;
    }
    if let Some(pages) = app.get_webview(title_bar::PAGES) {
        match url::Url::parse(&overview(shared, shown)) {
            Ok(url) => {
                if let Err(error) = pages.navigate(url) {
                    tracing::warn!(%error, "the window could not open the overview");
                }
            }
            Err(error) => tracing::error!(%error, "the overview's address is not valid"),
        }
    }
}

/// Makes the frameless window, hidden, adds the bar and the pages, then
/// shows it with the keyboard on the pages.
fn make(app: &AppHandle, url: url::Url, bar: url::Url, origin: Origin) -> tauri::Result<()> {
    let window = tauri::window::WindowBuilder::new(app, LABEL)
        .title(tr("admin-analyte-2"))
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .decorations(false)
        .shadow(true)
        .visible(false)
        .build()?;
    let size = window.inner_size()?;
    let (top, below) = title_bar::split(size.width, size.height, window.scale_factor()?);
    let bar = window.add_child(bar_webview(bar), position(top), extent(top))?;
    let pages = window.add_child(
        pages_webview(app, url, origin),
        position(below),
        extent(below),
    )?;
    // Tauri adds its top-edge resize strip above the webviews now there are some.
    window.set_resizable(true)?;
    add_frame(&window, [&bar, &pages]);
    window.show()?;
    pages.set_focus()?;
    Ok(())
}

/// Adds what the bar's page cannot do itself (ADR 0022): Alt+Space for the
/// window menu in both webviews, the menu's items for a normal window, and
/// the native maximize button for snap layouts, made on the window's thread
/// and placed at once. The window works without them, so a failure is logged
/// and the window still opens.
fn add_frame(window: &Window, webviews: [&Webview; 2]) {
    for webview in webviews {
        if let Err(error) = frame::menu_on_alt_space(webview, window) {
            tracing::warn!(%error, "Alt+Space will not open the window menu");
        }
    }
    let owner = window.clone();
    let queued = window.run_on_main_thread(move || {
        // A new window is at its normal size.
        match_window_menu(&owner, Sizing::Normal);
        let follower = owner.clone();
        let added = frame::add_maximize_button(&owner, move |mouse| follow_mouse(&follower, mouse))
            .and_then(|()| {
                let size = owner.inner_size().map_err(frame::Error::Tauri)?;
                let scale = owner.scale_factor().map_err(frame::Error::Tauri)?;
                place_maximize_button(&owner, size, scale)
            });
        if let Err(error) = added {
            tracing::warn!(%error, "the maximize button will not offer snap layouts");
        }
    });
    if let Err(error) = queued {
        tracing::warn!(%error, "the maximize button will not offer snap layouts");
    }
}

/// Lays the native maximize button over the bar's in a client area of `size`.
fn place_maximize_button(
    window: &Window,
    size: PhysicalSize<u32>,
    scale: f64,
) -> Result<(), frame::Error> {
    let (top, _) = title_bar::split(size.width, size.height, scale);
    // A window too small for the buttons gets none: it shrinks to nothing.
    let area = title_bar::maximize_button(size.width, top, scale).unwrap_or(title_bar::Area {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    });
    frame::place_maximize_button(window, area)
}

/// Shows where the mouse is on the maximize button, and maximizes or
/// restores the window on a click, as the bar's own button would.
fn follow_mouse(window: &Window, mouse: Mouse) {
    let Some(shared) = window.try_state::<std::sync::Arc<Shared>>() else {
        return;
    };
    let mut clicked = false;
    if let Some(view) = shared.update_bar(|view| {
        clicked = view.pointer.clicks(mouse);
        view.pointer = view.pointer.after(mouse);
    }) {
        show_in(window, &view);
    }
    if clicked {
        let toggled = match window.is_maximized() {
            Ok(true) => window.unmaximize(),
            Ok(false) => window.maximize(),
            Err(error) => Err(error),
        };
        if let Err(error) = toggled {
            tracing::warn!(%error, "the window could not be maximized or restored");
        }
    }
}

fn position(place: Place) -> PhysicalPosition<u32> {
    PhysicalPosition::new(0, place.y)
}

fn extent(place: Place) -> PhysicalSize<u32> {
    PhysicalSize::new(place.width, place.height)
}

/// Keeps the bar and the pages in place as the window changes size or
/// screen, and the middle button's name with whether it is maximized.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Tauri's WindowEvent is non-exhaustive; only size changes move the webviews"
)]
pub(super) fn on_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    let size = match event {
        WindowEvent::Resized(size) => *size,
        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => *new_inner_size,
        WindowEvent::CloseRequested { .. } => {
            tracing::info!("the window was closed; the app stays in the tray");
            return;
        }
        _ => return,
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let (top, below) = title_bar::split(size.width, size.height, scale);
    for (label, place) in [(title_bar::LABEL, top), (title_bar::PAGES, below)] {
        if let Some(webview) = window.get_webview(label)
            && let Err(error) = webview.set_bounds(Rect {
                position: position(place).into(),
                size: extent(place).into(),
            })
        {
            tracing::warn!(%error, label, "a webview could not be fitted to the window");
        }
    }
    match place_maximize_button(window, size, scale) {
        Ok(()) => {}
        // The first sizes come before the button is made, and it is placed then.
        Err(frame::Error::Missing) => tracing::debug!("the maximize button is not made yet"),
        Err(error) => tracing::warn!(%error, "the maximize button could not follow the window"),
    }
    match window.is_maximized() {
        Ok(maximized) => {
            if let Some(shared) = window.try_state::<std::sync::Arc<Shared>>()
                && let Some(view) = shared.update_bar(|view| view.sizing = Sizing::of(maximized))
            {
                show_in(window, &view);
            }
            match_window_menu(window, Sizing::of(maximized));
        }
        Err(error) => tracing::warn!(%error, "whether the window is maximized could not be read"),
    }
}

/// Greys what the window menu cannot do at `sizing` (ADR 0022).
fn match_window_menu(window: &Window, sizing: Sizing) {
    if let Err(error) = frame::match_window_menu(window, sizing) {
        tracing::warn!(%error, "the window menu may offer what the window cannot do");
    }
}

/// Shows the engine's state in the title bar, if the window is open.
pub(super) fn show_state(app: &AppHandle, shared: &Shared, shown: Shown) {
    if let Some(view) = shared.update_bar(|view| view.shown = shown)
        && let Some(window) = app.get_window(LABEL)
    {
        show_in(&window, &view);
    }
}

/// Shows the last notification's line in the title bar, if the window is
/// open and the line changed.
pub(super) fn show_note(app: &AppHandle, shared: &Shared, note: String) {
    if let Some(view) = shared.update_bar(|view| view.note = note)
        && let Some(window) = app.get_window(LABEL)
    {
        show_in(&window, &view);
    }
}

fn show_in<R: Runtime>(window: &Window<R>, view: &View) {
    if let Some(bar) = window.get_webview(title_bar::LABEL) {
        show(&bar, view);
    }
}

/// Runs `view`'s script in the bar.
fn show<R: Runtime>(bar: &Webview<R>, view: &View) {
    if let Err(error) = bar.eval(view.script()) {
        tracing::warn!(%error, "the title bar could not be updated");
    }
}

/// Opens `address` in the pages, if the window is open: where a Windows
/// sign-in leads. The address is never logged, as it may hold a code.
pub(super) fn go(app: &AppHandle, address: &str) {
    let Some(pages) = app.get_webview(title_bar::PAGES) else {
        return;
    };
    match url::Url::parse(address) {
        Ok(url) => {
            if let Err(error) = pages.navigate(url) {
                tracing::warn!(%error, "the window could not open the sign-in's page");
            }
        }
        Err(error) => tracing::error!(%error, "the sign-in's page address is not valid"),
    }
}

/// Moves the pages to where `shown` wants them: the engine's once it
/// answers, the Engine off screen once it does not, and a fresh Engine off
/// screen when what it shows `changed`.
pub(super) fn follow(app: &AppHandle, shared: &Shared, shown: Shown, changed: bool) {
    let Some(pages) = app.get_webview(title_bar::PAGES) else {
        return;
    };
    let current = match pages.url() {
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
            if let Err(error) = pages.navigate(url) {
                tracing::warn!(%error, "the window could not change page");
            }
        }
        Err(error) => tracing::error!(%error, "the window's address is not valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;

    /// The window commands the title bar may run, and nothing else may.
    const BAR_COMMANDS: [&str; 3] = [
        "plugin:window|minimize",
        "plugin:window|toggle_maximize",
        "plugin:window|close",
    ];

    /// Commands nobody is given: other window and webview changes, reading
    /// state, events, and the app's plugins. Dragging is Windows' own through
    /// the bar's app-region (ADR 0022), so Tauri's drag commands are refused.
    const OTHER_COMMANDS: [&str; 11] = [
        "plugin:window|start_dragging",
        "plugin:window|internal_toggle_maximize",
        "plugin:window|is_maximized",
        "plugin:window|set_title",
        "plugin:window|destroy",
        "plugin:window|set_size",
        "plugin:webview|create_webview",
        "plugin:webview|set_webview_focus",
        "plugin:event|listen",
        "plugin:app|version",
        "plugin:opener|open_url",
    ];

    /// A child webview as Tauri's test helper takes one.
    struct Child<'a>(&'a Webview<MockRuntime>);

    impl AsRef<Webview<MockRuntime>> for Child<'_> {
        fn as_ref(&self) -> &Webview<MockRuntime> {
            self.0
        }
    }

    fn invoke(
        webview: &Webview<MockRuntime>,
        command: &str,
        from: &str,
    ) -> Result<(), serde_json::Value> {
        get_ipc_response(
            &Child(webview),
            InvokeRequest {
                cmd: command.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: from.parse().expect("a test address"),
                body: InvokeBody::default(),
                headers: tauri::http::HeaderMap::default(),
                invoke_key: INVOKE_KEY.into(),
            },
        )
        .map(|_| ())
    }

    /// Whether Tauri's permission check refused `command`, rather than the
    /// command failing for another reason.
    fn refused(webview: &Webview<MockRuntime>, command: &str, from: &str) -> bool {
        match invoke(webview, command, from) {
            Err(serde_json::Value::String(why)) => why.contains(" not allowed"),
            Ok(()) | Err(_) => false,
        }
    }

    // ADR 0019 (title bar, security): built from the app's real capabilities,
    // the window's two webviews differ only in what Tauri lets them run. The
    // engine's pages may run no command, from the engine or from the app's
    // own Engine off screen; the bar may run exactly its three window
    // commands, and only from the app's own origin.
    #[test]
    fn only_the_title_bar_may_run_its_window_commands() {
        let app = mock_builder()
            .register_uri_scheme_protocol(crate::engine::LOCAL_SCHEME, |_, _| {
                tauri::http::Response::new(Vec::new())
            })
            .build(tauri::generate_context!("tauri.conf.json", test = true))
            .expect("the app builds on the mock runtime");
        let window = tauri::window::WindowBuilder::new(&app, LABEL)
            .build()
            .expect("the window");
        let (top, below) = title_bar::split(1280, 800, 1.0);
        let bar = window
            .add_child(
                bar_webview(bar_address().parse().expect("the bar's address")),
                position(top),
                extent(top),
            )
            .expect("the bar");
        let engine = "http://127.0.0.1:8080/";
        let pages = window
            .add_child(
                pages_webview(
                    app.handle(),
                    engine.parse().expect("the engine's address"),
                    Origin::Engine(8080),
                ),
                position(below),
                extent(below),
            )
            .expect("the pages");

        let off = engine_off();
        for command in BAR_COMMANDS.iter().chain(&OTHER_COMMANDS) {
            for from in [engine, off.as_str()] {
                assert!(
                    refused(&pages, command, from),
                    "the pages may run {command} from {from}"
                );
            }
        }
        let bar_page = bar_address();
        for command in OTHER_COMMANDS {
            assert!(
                refused(&bar, command, &bar_page),
                "the bar may run {command}"
            );
        }
        for command in BAR_COMMANDS {
            assert!(
                refused(&bar, command, engine),
                "the bar may run {command} from the engine's origin"
            );
        }
        // Close last: it closes the window.
        for command in BAR_COMMANDS {
            assert_eq!(invoke(&bar, command, &bar_page), Ok(()), "{command}");
        }
    }
}
