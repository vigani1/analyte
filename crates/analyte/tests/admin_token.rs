//! Tests plan11 plan12 cli prints rotated token once without logging it.
use analyte::Error;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::process::Command;

#[test]
fn plan11_plan12_cli_prints_rotated_token_once_without_logging_it() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("machine.toml"),
        include_str!("../../core/config/examples/hl7.toml"),
    )?;
    std::fs::write(
        directory.path().join("site.toml"),
        include_str!("../../core/config/examples/site.toml"),
    )?;
    let config = directory.path().join("program.toml");
    std::fs::write(
        &config,
        "schema_version = 1\ndata_directory = 'data'\n[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n",
    )?;
    let run = |rotate: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_analyte"));
        command
            .arg("--config")
            .arg(&config)
            .args(["admin", "token"]);
        if rotate {
            command.arg("--rotate");
        }
        command.env("RUST_LOG", "trace").output()
    };
    let refused = run(false)?;
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    assert!(!directory.path().join("data/admin-token").exists());
    let first = run(true)?;
    assert!(first.status.success());
    let stdout = String::from_utf8(first.stdout)?;
    assert_eq!(stdout.lines().count(), 1);
    let token = stdout.trim_end();
    assert_eq!(STANDARD.decode(token)?.len(), 32);
    assert!(!String::from_utf8(first.stderr)?.contains(token));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("data/admin-token"))?,
        token
    );
    let second = run(true)?;
    assert!(second.status.success());
    let stdout = String::from_utf8(second.stdout)?;
    assert_ne!(stdout.trim_end(), token);
    assert!(!String::from_utf8(second.stderr)?.contains(stdout.trim_end()));
    Ok(())
}
