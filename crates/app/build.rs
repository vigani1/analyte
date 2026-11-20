//! Prepares the Tauri app on Windows: its configuration, and the icon and "Analyte" description compiled into analyte-app.exe.
fn main() {
    // The app's own pages need no Tauri commands, so no capability is granted.
    #[cfg(windows)]
    tauri_build::build();
}
