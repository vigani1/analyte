//! Checks the language selected through the command-line executable.
#![forbid(unsafe_code)]

// ADR 0039: the explicit tag scopes the command's diagnostic output.
#[test]
fn doctor_renders_in_albanian() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .args(["--lang", "sq", "--config"])
        .arg(directory.path().join("missing.toml"))
        .arg("doctor")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8(output.stdout)?;
    assert!(diagnostics.contains("Skedari i programit"), "{diagnostics}");
    assert!(
        diagnostics.contains("Diagnostika: 1 dështim dhe 0 paralajmërime."),
        "{diagnostics}"
    );
    Ok(())
}
