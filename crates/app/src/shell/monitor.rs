//! The one task that watches the engine: asks its `/status` every few seconds, keeps the tray, the window and the Engine off screen in step, notifies when the state gets worse, and runs the window and Start engine commands.
//!
//! Every wait is bounded: a question by `status::TIMEOUT`, an `sc.exe` call
//! by `service::LIMIT`. When the app quits, the task is cancelled within
//! the shell's stop time: a question in flight is dropped, and a running
//! `sc.exe` is stopped. A start the service manager already accepted goes
//! on, which is what was asked; a timeout never proves nothing happened.
use super::{Command, Shared, tray, window};
use crate::engine::{service, status::Client};
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
    let mut ticks = tokio::time::interval(INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {
                if let Some(client) = &client {
                    check(app, shared, client, &mut watch).await;
                }
            }
            command = commands.recv() => match command {
                Some(Command::Open) => window::open(app, shared, watch.shown()),
                Some(Command::StartEngine) => {
                    start(app, shared, watch.shown()).await;
                    ticks.reset_immediately();
                }
                None => return,
            },
        }
    }
}

/// Asks the engine once and brings everything in step with the answer.
async fn check(app: &AppHandle, shared: &Shared, client: &Client, watch: &mut Watch) {
    let reply = client.ask().await;
    let now = jiff::Timestamp::now();
    let change = watch.observe(reply, now);
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
    if let Some(change) = change {
        tracing::info!(from = ?change.from, to = ?change.to, "engine state changed");
        tray::show(app, change.to);
        if let Some(notice) = change.notice() {
            notify(app, notice);
        }
    }
    window::follow(app, shared, watch.shown(), changed);
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

fn notify(app: &AppHandle, notice: Notice) {
    use tauri_plugin_notification::NotificationExt as _;
    let shown = app
        .notification()
        .builder()
        .title(notice.title)
        .body(notice.body)
        .show();
    match shown {
        // Windows may still hold it back (notifications off, Do not disturb).
        Ok(()) => tracing::info!(title = notice.title, "notification handed to Windows"),
        Err(error) => tracing::warn!(%error, "a notification could not be shown"),
    }
}
