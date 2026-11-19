//! Compiles the app icon into analyte.exe when building for Windows, so Explorer, Task Manager and the service list show it.
fn main() -> Result<(), embed_resource::CompilationResult> {
    println!("cargo::rerun-if-changed=analyte.rc");
    println!("cargo::rerun-if-changed=assets/icons/analyte.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("analyte.rc", embed_resource::NONE).manifest_optional()?;
    }
    Ok(())
}
