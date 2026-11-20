//! Prepares the Tauri app on Windows: its configuration, the title bar's one capability, the icon and "Analyte" description compiled into analyte-app.exe, and the Windows manifest in the app and its tests alike.
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Only the title bar's webview gets a capability (`capabilities/title-bar.json`):
    // its window commands. The engine's pages get none.
    #[cfg(windows)]
    {
        let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        let windows = if msvc {
            embed_manifest();
            tauri_build::WindowsAttributes::new_without_app_manifest()
        } else {
            tauri_build::WindowsAttributes::new()
        };
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;
    }
    Ok(())
}

/// Links the Common Controls 6 manifest Tauri needs into every program of
/// this crate, the test programs included: tauri-build adds it to the app
/// alone, and a test using Tauri then fails to start. Tauri's own tests do
/// the same.
#[cfg(windows)]
fn embed_manifest() {
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
