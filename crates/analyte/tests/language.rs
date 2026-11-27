//! Checks the language selected through the command-line executable.
#![forbid(unsafe_code)]

// ADR 0039: the explicit tag scopes the command and its help to that language.
#[test]
fn overview_help_renders_in_albanian() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .args(["--lang", "sq", "overview", "--help"])
        .output()?;
    assert!(output.status.success(), "{:?}", output.stderr);
    let help = String::from_utf8(output.stdout)?;
    assert!(help.contains("Përmbledhja"), "{help}");
    assert!(help.contains("--json"), "{help}");
    Ok(())
}
