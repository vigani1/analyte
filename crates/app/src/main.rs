//! Starts the Analyte desktop app: the window and the tray on Windows; elsewhere it says it is Windows only.
#![forbid(unsafe_code)]
#![cfg_attr(not(test), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    analyte_app::shell::run()
}

#[cfg(not(windows))]
#[expect(
    clippy::print_stderr,
    reason = "the only output of a program that cannot run here"
)]
fn main() -> std::process::ExitCode {
    eprintln!(
        "{}",
        analyte_i18n::text("desktop-the-analyte-desktop-app-runs-on-windows-only-open")
    );
    std::process::ExitCode::FAILURE
}
