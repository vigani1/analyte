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

// ADR 0039 and Unicode CLDR plural rules: French uses the singular for 0.
#[test]
fn doctor_renders_in_french() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .args(["--lang", "fr", "--config"])
        .arg(directory.path().join("missing.toml"))
        .arg("doctor")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8(output.stdout)?;
    assert!(
        diagnostics.contains("Fichier du programme"),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("Diagnostic\u{a0}: 1 échec et 0 avertissement."),
        "{diagnostics}"
    );
    Ok(())
}

// ADR 0039: a Spanish tag uses the Spanish catalog and its plural rules.
#[test]
fn doctor_renders_in_spanish() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .args(["--lang", "es", "--config"])
        .arg(directory.path().join("missing.toml"))
        .arg("doctor")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8(output.stdout)?;
    assert!(
        diagnostics.contains("Archivo del programa"),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("Diagnóstico: 1 fallo y 0 avisos."),
        "{diagnostics}"
    );
    Ok(())
}
