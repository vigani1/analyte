//! The one task that watches the engine: asks its `/status` every few seconds, keeps the tray, the window, its title bar and the Engine off screen in step, notifies when the state gets worse or something new needs someone, and runs the window, Start engine and Windows sign-in commands.
//!
//! Every wait is bounded: a question by `status::TIMEOUT`, an `sc.exe` call
//! by `service::LIMIT`, a sign-in by `pipe::LIMITS` (5 s in all). When the
//! app quits, the task is cancelled within the shell's stop time: a question
//! in flight is dropped, a running `sc.exe` is stopped and the sign-in pipe
//! is closed. A start the service manager already accepted goes on, which is
//! what was asked; a code the engine already made expires unused after 20 s.
//! A timeout never proves nothing happened.
use super::{Command, Shared, tray, window};
use crate::engine::{service, status::Client};
use crate::sign_in::{self, Guard, Trigger, pipe};
use crate::watch::{Notice, Shown, Watch};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::{mpsc, watch};

/// How often the engine is asked.
pub(super) const INTERVAL: Duration = Duration::from_secs(5);

/// Runs until `stop` turns true or every command sender is gone.
pub(super) async fn run(
    app: AppHandle,
    shared: Arc<Shared>,
    commands: mpsc::Receiver<Command>,
    mut stop: watch::Receiver<bool>,
) {
    tokio::select! {
        _ = stop.wait_for(|stopped| *stopped) => {}
        () = work(&app, &shared, commands) => {}
    }
}

async fn work(app: &AppHandle, shared: &Shared, mut commands: mpsc::Receiver<Command>) {
    let client = match &shared.engine {
        Ok(engine) => match Client::new(engine) {
            Ok(client) => Some(client),
            Err(error) => {
                tracing::error!(%error, "the app cannot ask the engine");
                shared.update_off(|off| {
                    off.settings = Some(format!("The app cannot make HTTP requests: {error}"));
                });
                None
            }
        },
        Err(_) => None,
    };
    let mut watch = Watch::new();
    let mut guard = Guard::default();
    let mut ticks = tokio::time::interval(INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {
                if let Some(client) = &client {
                    check(app, shared, client, &mut watch, &mut guard).await;
                }
            }
            command = commands.recv() => match command {
                Some(Command::Open) => {
                    // A person opened the window, so it signs in at once.
                    guard.forget();
                    window::open(app, shared, watch.shown());
                }
                Some(Command::OpenOverview) => {
                    guard.forget();
                    window::open_overview(app, shared, watch.shown());
                }
                Some(Command::SignIn { next, trigger }) => {
                    sign_in(app, shared, &mut guard, &next, trigger).await;
                }
                Some(Command::StartEngine) => {
                    start(app, shared, watch.shown()).await;
                    ticks.reset_immediately();
                }
                None => return,
            },
        }
    }
}

/// Asks the engine once and brings everything in step with the answer; the
/// guard hears of a change before the window follows it, so an engine that
/// answers again signs the window in at once.
async fn check(
    app: &AppHandle,
    shared: &Shared,
    client: &Client,
    watch: &mut Watch,
    guard: &mut Guard,
) {
    let reply = client.ask().await;
    let now = jiff::Timestamp::now();
    let seen = watch.observe(reply, now);
    let running = matches!(watch.shown(), Shown::Running(_));
    // Only while the engine does not answer: the service manager says why.
    let service = if running {
        None
    } else {
        Some(service::query().await)
    };
    let changed = shared.update_off(|off| {
        off.last_answer = watch.last_answer();
        off.reply = watch.last_reply().cloned();
        off.service = service;
        if running {
            off.noticed = None;
            off.started = None;
        } else if off.noticed.is_none() {
            off.noticed = Some(now);
        }
    });
    if let Some(change) = seen.change {
        tracing::info!(from = ?change.from, to = ?change.to, "engine state changed");
        guard.follow(change);
        tray::show(app, change.to);
        window::show_state(app, shared, change.to);
    }
    if let Some(notice) = &seen.notice {
        notify(app, notice);
    }
    let note = watch
        .recap()
        .and_then(|recap| recap.line(now, &jiff::tz::TimeZone::system()))
        .unwrap_or_default();
    window::show_note(app, shared, note);
    window::follow(app, shared, watch.shown(), changed);
}

/// Signs the pages in with Windows when the guard allows, then opens where
/// the answer leads: the code, the refusal's sentence or the key form.
async fn sign_in(
    app: &AppHandle,
    shared: &Shared,
    guard: &mut Guard,
    next: &str,
    trigger: Trigger,
) {
    let Ok(engine) = &shared.engine else {
        return;
    };
    let pipe = sign_in::pipe_name(engine);
    let now = std::time::Instant::now();
    let landing = sign_in::land(guard, &pipe, next, trigger, now, pipe::LIMITS).await;
    window::go(app, &landing.address(engine));
}

/// Asks the service manager to start the engine, and shows what came of it.
async fn start(app: &AppHandle, shared: &Shared, shown: Shown) {
    // A page that was open before the engine answered again asks for nothing.
    if matches!(shown, Shown::Running(_)) {
        shared.update_off(|off| off.starting = false);
        return;
    }
    shared.update_off(|off| off.starting = true);
    let started = service::start().await;
    tracing::info!(?started, "start engine requested");
    let changed = shared.update_off(|off| {
        off.starting = false;
        off.started = Some(started);
    });
    window::follow(app, shared, shown, changed);
}

fn notify(app: &AppHandle, notice: &Notice) {
    use tauri_plugin_notification::NotificationExt as _;
    let shown = app
        .notification()
        .builder()
        .title(notice.title)
        .body(&notice.body)
        .show();
    match shown {
        // Windows may still hold it back (notifications off, Do not disturb).
        Ok(()) => tracing::info!(title = notice.title, "notification handed to Windows"),
        Err(error) => tracing::warn!(%error, "a notification could not be shown"),
    }
}
