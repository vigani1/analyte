//! The Analyte desktop app: one window on the engine's admin pages and a tray icon with the engine's state (ADR 0019).
//!
//! The app never runs the engine and never holds the admin key: it opens the
//! engine's pages at `http://127.0.0.1:<admin port>/`, signs people in with
//! their Windows account through the engine's sign-in pipe (ADR 0021), where
//! the key form stays the way back in, and it reads the engine's public
//! `GET /status` for the tray. Everything that decides what the app shows lives here, apart
//! from the Windows shell, so it is tested on every platform:
//!
//! - [`engine`]: where the engine is and what its `/status` answered;
//! - [`watch`]: the state the tray shows, and when a change earns a notification;
//! - [`navigation`]: which addresses the window may show;
//! - [`sign_in`]: Windows sign-in: when to ask the pipe and where its answer leads;
//! - [`local`]: the app's own pages (Engine off, the title bar) and their files;
//! - [`title_bar`]: what the window's own title bar shows, where it and its
//!   maximize button sit, and what its window menu offers.
// ADR 0022: only the shell's `frame` module may expect unsafe_code.
#![deny(unsafe_code)]

#[path = "../../analyte/src/admin/view/status_words.rs"]
mod status_words;

pub mod engine;
pub mod local;
pub mod navigation;
pub mod sign_in;
pub mod title_bar;
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

    // ADR 0019 (title bar): the window's two webviews use Tauri's multi-
    // webview API, which is behind its `unstable` feature and may change in
    // any release. Tauri and the crates that carry that API are pinned; an
    // upgrade fails here until the title bar has been tried on the new
    // versions and ADR 0019 names them.
    #[test]
    fn the_unstable_tauri_versions_are_the_reviewed_ones() {
        let manifest = include_str!("../../../Cargo.toml");
        assert!(
            manifest.contains(r#"tauri = { version = "=2.12.7","#)
                && manifest.contains(r#""image-png", "unstable"] }"#),
            "the workspace no longer pins tauri =2.12.7 with `unstable`"
        );
        let lock = include_str!("../../../Cargo.lock");
        let adr = include_str!("../../../docs/adr/0019-desktop-app.md");
        for (name, version) in [
            ("tauri", "2.12.7"),
            ("tauri-runtime", "2.12.4"),
            ("tauri-runtime-wry", "2.12.5"),
            ("wry", "0.57.3"),
        ] {
            let entry = format!("name = \"{name}\"\nversion = \"{version}\"\n");
            assert!(
                lock.replace("\r\n", "\n").contains(&entry),
                "Cargo.lock has another {name} than the reviewed {version}"
            );
            assert!(
                adr.contains(&format!("{name} {version}")),
                "ADR 0019 does not name {name} {version}"
            );
        }
    }
}
