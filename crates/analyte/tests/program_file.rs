//! Tests which program file a command reads when `--config` is not given.
//!
//! Each case runs the real program in its own temporary folder with a broken
//! program file, so the command stops at loading it and the error and the note
//! on standard error say which file it read. On Windows, `ProgramData` points
//! at a temporary folder, so the PC's own installation is never read.
type Outcome = Result<(), Box<dyn std::error::Error + Send + Sync>>;
use std::path::Path;
use std::process::{Command, Output};

/// `analyte parked list` in `folder`, without `--config` and `ANALYTE_CONFIG`.
fn run(folder: &Path, program_data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_analyte"));
    command
        .args(["parked", "list"])
        .current_dir(folder)
        .env_remove("ANALYTE_CONFIG")
        .env("ProgramData", program_data);
    command
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A file that is not TOML, so loading it fails and nothing else happens.
fn broken(path: &Path) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, "not a program file\n")
}

// Owner request 2026-11-21 (engine-only install): --config, then ANALYTE_CONFIG,
// then analyte.toml in the current folder, then the installed program file.
#[test]
fn the_current_folder_comes_before_the_installed_file() -> Outcome {
    let folder = tempfile::tempdir()?;
    let program_data = tempfile::tempdir()?;
    broken(&folder.path().join("analyte.toml"))?;
    broken(&program_data.path().join(r"Analyte\config\analyte.toml"))?;
    let output = run(folder.path(), program_data.path()).output()?;
    assert!(!output.status.success());
    assert!(
        stderr(&output).starts_with("Using analyte.toml (in the current folder).\n"),
        "{}",
        stderr(&output)
    );
    // The variable wins over both, and is read even though it does not exist.
    let named = folder.path().join("named.toml");
    let output = run(folder.path(), program_data.path())
        .env("ANALYTE_CONFIG", &named)
        .output()?;
    assert!(!output.status.success());
    assert!(
        stderr(&output).starts_with(&format!(
            "Using {} (named by ANALYTE_CONFIG).\n",
            named.display()
        )),
        "{}",
        stderr(&output)
    );
    // --config wins over everything, without a note.
    let output = run(folder.path(), program_data.path())
        .env("ANALYTE_CONFIG", &named)
        .args(["--config", "chosen.toml"])
        .output()?;
    assert!(!output.status.success());
    assert!(!stderr(&output).contains("Using "), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    Ok(())
}

// Owner request 2026-11-21 (engine-only install): on an installed PC every
// command reads %ProgramData%\Analyte\config\analyte.toml without --config.
#[cfg(windows)]
#[test]
fn the_installed_file_is_read_without_config() -> Outcome {
    let folder = tempfile::tempdir()?;
    let program_data = tempfile::tempdir()?;
    let installed = program_data.path().join(r"Analyte\config\analyte.toml");
    broken(&installed)?;
    let output = run(folder.path(), program_data.path()).output()?;
    assert!(!output.status.success());
    assert!(
        stderr(&output).starts_with(&format!(
            "Using {} (the installed program file).\n",
            installed.display()
        )),
        "{}",
        stderr(&output)
    );
    // The note goes to standard error only; standard output stays the command's.
    assert!(output.stdout.is_empty());
    Ok(())
}

// Owner request 2026-11-21 (engine-only install): with no file anywhere, the
// error names every place looked and how to choose one.
#[cfg(windows)]
#[test]
fn nothing_found_names_every_place_looked() -> Outcome {
    let folder = tempfile::tempdir()?;
    let program_data = tempfile::tempdir()?;
    let output = run(folder.path(), program_data.path()).output()?;
    assert!(!output.status.success());
    let installed = program_data.path().join(r"Analyte\config\analyte.toml");
    assert!(
        stderr(&output).contains(&format!(
            "no program file found; looked for analyte.toml, {}. Pass --config <file> or set ANALYTE_CONFIG",
            installed.display()
        )),
        "{}",
        stderr(&output)
    );
    Ok(())
}
