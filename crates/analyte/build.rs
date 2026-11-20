//! Compiles the app icon and the version details into analyte.exe when building for Windows, so Explorer, Task Manager and the service list show "Analyte engine" with the Aperture mark.
fn main() -> Result<(), embed_resource::CompilationResult> {
    println!("cargo::rerun-if-changed=analyte.rc");
    println!("cargo::rerun-if-changed=analyte.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // The version comes from the workspace, so it is written in one place.
        let part = |name: &str| std::env::var(name).unwrap_or_else(|_| "0".into());
        let macros = [
            format!("ANALYTE_MAJOR={}", part("CARGO_PKG_VERSION_MAJOR")),
            format!("ANALYTE_MINOR={}", part("CARGO_PKG_VERSION_MINOR")),
            format!("ANALYTE_PATCH={}", part("CARGO_PKG_VERSION_PATCH")),
            format!("ANALYTE_VERSION=\"{}\"", env!("CARGO_PKG_VERSION")),
        ];
        embed_resource::compile("analyte.rc", macros).manifest_optional()?;
    }
    Ok(())
}
