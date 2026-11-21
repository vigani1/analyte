//! Runs the app on Windows with Tauri: one instance, the tray, the window on demand, the app's own pages, and the one task that watches the engine.
//!
//! Everything that changes the window or the tray goes through the monitor
//! task (`monitor`), which owns the engine watch; menu clicks, the second
//! instance and notification clicks only send it a `Command`. That keeps
//! one owner for every change and builds windows off the event loop's thread,
//! as `WebView2` requires.
mod autostart;
#[expect(
    unsafe_code,
    reason = "ADR 0022: the title bar's snap layouts and window menu FFI"
)]
mod frame;
mod log;
mod monitor;
mod tray;
mod window;

use crate::engine::{Engine, LOCAL_SCHEME};
use crate::local::{self, Off};
use crate::title_bar::View;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use tauri::Manager as _;
use tokio::sync::{mpsc, watch};

/// The argument the sign-in entry passes, so the app starts in the tray only.
pub const TRAY_ONLY: &str = "--tray";

/// What the monitor task is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Show the window, making it if it was closed.
    Open,
    /// Ask the service manager to start the engine.
    StartEngine,
    /// Sign the pages in with Windows, then open `next` (ADR 0021 §6).
    SignIn {
        /// The page to land on; the engine checks it.
        next: String,
        /// Who asked, for the once-per-30-s guard.
        trigger: crate::sign_in::Trigger,
    },
}

/// How many commands may wait; more are dropped, as they repeat one another.
const COMMANDS: usize = 8;

/// What the app's pages and the monitor share. Each lock guards only the
/// snapshot one of the app's own pages shows, and is never held across a
/// wait.
pub(crate) struct Shared {
    /// The engine, or why the settings file could not name it.
    pub(crate) engine: Result<Engine, String>,
    pub(crate) off: Mutex<Off>,
    /// What the title bar shows: the engine's state and the window's sizing.
    pub(crate) bar: Mutex<View>,
    pub(crate) commands: mpsc::Sender<Command>,
}

impl Shared {
    /// A copy of what the Engine off screen shows now.
    pub(crate) fn off(&self) -> Off {
        self.off
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Changes the Engine off snapshot; says whether it changed.
    pub(crate) fn update_off(&self, change: impl FnOnce(&mut Off)) -> bool {
        let mut off = self.off.lock().unwrap_or_else(PoisonError::into_inner);
        let before = off.clone();
        change(&mut off);
        *off != before
    }

    /// A copy of what the title bar shows now.
    pub(crate) fn bar(&self) -> View {
        *self.bar.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Changes what the title bar shows; gives the new view if it changed.
    pub(crate) fn update_bar(&self, change: impl FnOnce(&mut View)) -> Option<View> {
        let mut bar = self.bar.lock().unwrap_or_else(PoisonError::into_inner);
        let before = *bar;
        change(&mut bar);
        (*bar != before).then_some(*bar)
    }

    /// Hands `command` to the monitor; a full queue already holds one like it.
    pub(crate) fn send(&self, command: Command) {
        if let Err(error) = self.commands.try_send(command) {
            let why = error.to_string();
            // The page a sign-in returns to is left out of the log.
            let command = match error.into_inner() {
                Command::Open => "open",
                Command::StartEngine => "start engine",
                Command::SignIn { .. } => "sign in",
            };
            tracing::debug!(command, %why, "command not queued");
        }
    }
}

/// Runs the app until "Quit app"; the exit code says whether it could start.
pub fn run() -> ExitCode {
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
    log::start();
    let engine = folder
        .as_deref()
        .ok_or_else(|| "the app could not find its own folder".to_owned())
        .and_then(|folder| Engine::from_settings(folder).map_err(|error| error.to_string()));
    if let Err(why) = &engine {
        tracing::error!(%why, "the app's settings file could not be used");
    }
    let tray_only = std::env::args().any(|argument| argument == TRAY_ONLY);
    match build(engine, tray_only) {
        Ok(app) => {
            app.run(on_event);
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(%error, "the app could not start");
            ExitCode::FAILURE
        }
    }
}

/// The monitor task and its stop switch, owned by the app until it exits.
struct Owner {
    stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// How long the monitor may take to stop when the app quits: one question
/// or one `sc.exe` call is cancelled, never waited for.
const STOP_TIME: std::time::Duration = std::time::Duration::from_secs(2);

fn build(engine: Result<Engine, String>, tray_only: bool) -> tauri::Result<tauri::App> {
    let (commands, received) = mpsc::channel(COMMANDS);
    let origin = engine.as_ref().map(Engine::origin).ok();
    let shared = Arc::new(Shared {
        off: Mutex::new(Off {
            origin: origin.map(crate::engine::Origin::text).unwrap_or_default(),
            settings: engine.as_ref().err().cloned(),
            ..Off::default()
        }),
        engine,
        bar: Mutex::new(View::default()),
        commands,
    });
    let (stop, stopped) = watch::channel(false);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, arguments, _| {
            if !arguments.iter().any(|argument| argument == TRAY_ONLY) {
                app.state::<Arc<Shared>>().send(Command::Open);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .register_uri_scheme_protocol(LOCAL_SCHEME, |context, request| {
            let shared = context.app_handle().state::<Arc<Shared>>();
            local_page(
                &shared,
                local::Webview::of(context.webview_label()),
                &request,
            )
        })
        .on_window_event(window::on_event)
        .manage(Arc::clone(&shared))
        .setup(move |app| {
            tray::create(app.handle())?;
            notifications(app.handle());
            let task = tauri::async_runtime::spawn(monitor::run(
                app.handle().clone(),
                Arc::clone(&shared),
                received,
                stopped,
            ));
            app.manage(Owner {
                stop,
                task: Mutex::new(Some(task)),
            });
            if !tray_only {
                shared.send(Command::Open);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
}

/// Keeps the app in the tray when its window closes; stops the monitor when
/// "Quit app" ends it.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Tauri's RunEvent is non-exhaustive; every other event needs nothing here"
)]
fn on_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    match event {
        // Closing the window frees its webview; only "Quit app" passes a code.
        tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        tauri::RunEvent::Exit => {
            let owner = app.state::<Owner>();
            owner.stop.send_replace(true);
            let task = owner
                .task
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(task) = task {
                let joined = tauri::async_runtime::block_on(tokio::time::timeout(STOP_TIME, task));
                if joined.is_err() {
                    tracing::warn!("the monitor did not stop in time");
                }
            }
        }
        _ => {}
    }
}

/// A click on a notification opens the window.
fn notifications(app: &tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt as _;
    let handle = app.clone();
    let registered = app.notification().on_action(move |performed| {
        if performed.action_id() == "tap" {
            handle.state::<Arc<Shared>>().send(Command::Open);
        }
    });
    if let Err(error) = registered {
        tracing::warn!(%error, "notification clicks will not open the window");
    }
}

/// Answers one request to the app's own pages from `webview`.
fn local_page(
    shared: &Shared,
    webview: local::Webview,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let origin = request
        .headers()
        .get(tauri::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let asked = local::Request {
        method: request.method().as_str(),
        path: request.uri().path(),
        origin,
        webview,
    };
    let (answer, action) =
        local::answer(&asked, &shared.off(), shared.bar(), jiff::Timestamp::now());
    if action == Some(local::Action::StartEngine) {
        // Shown at once, so the page after the redirect already says "Starting…".
        shared.update_off(|off| off.starting = true);
        shared.send(Command::StartEngine);
    }
    let mut response = tauri::http::Response::builder()
        .status(answer.status)
        .header(tauri::http::header::CONTENT_TYPE, answer.kind)
        .header(tauri::http::header::CONTENT_SECURITY_POLICY, answer.policy)
        .header(
            tauri::http::header::CACHE_CONTROL,
            if answer.store {
                "max-age=3600"
            } else {
                "no-store"
            },
        );
    for (name, value) in local::HEADERS {
        response = response.header(name, value);
    }
    if let Some(location) = answer.location {
        response = response.header(tauri::http::header::LOCATION, location);
    }
    response
        .body(answer.body.into_owned())
        .unwrap_or_else(|error| {
            tracing::error!(%error, "an answer of the app's own pages could not be built");
            let mut failed = tauri::http::Response::new(Vec::new());
            *failed.status_mut() = tauri::http::StatusCode::INTERNAL_SERVER_ERROR;
            failed
        })
}
