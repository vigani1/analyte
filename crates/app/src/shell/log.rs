//! Keeps the app's log in the user's own `%LOCALAPPDATA%\Analyte\analyte-app.log`, at most about 1 MiB plus one older file.
//!
//! The app never sees patient data, and the log holds only what the app
//! did: state changes, refused addresses by scheme, errors.
use std::path::PathBuf;

/// When the log is moved aside at start.
const LIMIT: u64 = 1024 * 1024;

/// Starts logging to the file; without a usable folder the app runs unlogged.
pub(super) fn start() {
    let Some(folder) =
        std::env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join("Analyte"))
    else {
        return;
    };
    if std::fs::create_dir_all(&folder).is_err() {
        return;
    }
    let path = folder.join("analyte-app.log");
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > LIMIT) {
        // The one older file is replaced; a failure only keeps appending.
        let _ = std::fs::rename(&path, folder.join("analyte-app.old.log"));
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::sync::Mutex::new(file))
        .finish();
    // Only fails when a logger already exists, which then keeps logging.
    let _ = tracing::subscriber::set_global_default(subscriber);
}
