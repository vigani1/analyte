//! The Analyte desktop app: one window on the engine's admin pages and a tray icon with the engine's state (ADR 0019).
//!
//! The app never runs the engine and never holds the admin key: it opens the
//! engine's pages at `http://127.0.0.1:<admin port>/`, where people sign in
//! with the normal form, and it reads the engine's public `GET /status` for
//! the tray. Everything that decides what the app shows lives here, apart
//! from the Windows shell, so it is tested on every platform:
//!
//! - [`engine`]: where the engine is and what its `/status` answered;
//! - [`watch`]: the state the tray shows, and when a change earns a notification;
//! - [`navigation`]: which addresses the window may show;
//! - [`local`]: the app's own Engine off screen and its files.
#![forbid(unsafe_code)]

pub mod engine;
pub mod local;
pub mod navigation;
pub mod watch;

#[cfg(windows)]
pub mod shell;

#[cfg(test)]
mod tests {
    // ADR 0019: the version Windows shows for analyte-app.exe comes from
    // tauri.conf.json, which must follow the workspace version.
    #[test]
    fn the_file_version_is_the_workspace_version() {
        let config = include_str!("../tauri.conf.json");
        let line = format!("\"version\": \"{}\",", env!("CARGO_PKG_VERSION"));
        assert!(config.contains(&line), "tauri.conf.json lacks {line}");
    }
}
