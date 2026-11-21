//! Doctor's checks: FAIL for a program file that cannot be used, and with the engine stopped what it finds for the store, a machine's port and a lab app, with the exit codes scripts read.
use super::super::*;
use super::{closed_port, program_file};
use crate::admin::command::doctor;

// Owner request 2026-11-21: doctor is read-only and says FAIL for a program
// file that cannot be used, with exit code 2.
#[tokio::test(flavor = "multi_thread")]
async fn doctor_fails_a_bad_program_file() -> Result<(), Error> {
    let folder = tempfile::tempdir()?;
    let file = folder.path().join("program.toml");
    std::fs::write(&file, "schema_version = 'not a number'\n")?;
    let checks = doctor::checks(&file).await;
    let [check] = checks.as_slice() else {
        return Err("only the program file is checked".into());
    };
    assert_eq!(check.level, doctor::Level::Fail);
    assert_eq!(check.what, "Program file");
    assert!(check.fix.is_some());
    assert_eq!(doctor::worst(&checks).exit_code(), 2);
    Ok(())
}

// Owner request 2026-11-21: with the engine stopped doctor says so, warns
// that no store exists yet (and creates none), finds the machine's port free,
// and fails a lab app that gives no HTTPS answer and one without keys. It
// contacts only loopback addresses here.
#[tokio::test(flavor = "multi_thread")]
async fn doctor_reports_a_stopped_engine_an_unreachable_lab_app_and_missing_keys()
-> Result<(), Error> {
    let folder = crate::admin::private_tempdir()?;
    let root = folder.path();
    let machine = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .to_string();
    std::fs::write(
        root.join("machine.toml"),
        include_str!("../../../../../../core/config/examples/hl7.toml"),
    )?;
    let lab_app = format!("https://127.0.0.1:{}", closed_port()?);
    std::fs::write(
        root.join("site.toml"),
        include_str!("../../../../../../core/config/examples/site.toml")
            .replace("127.0.0.1:5600", &machine)
            .replace("https://lab.example.invalid", &lab_app),
    )?;
    let data = root.join("data");
    std::fs::create_dir(&data)?;
    let file = program_file(
        root,
        &data,
        Some(closed_port()?),
        "[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n",
    )?;
    let checks = doctor::checks(&file).await;
    let find = |what: &str, part: &str| {
        checks
            .iter()
            .find(|check| check.what == what && check.found.contains(part))
            .map(|check| check.level)
    };
    assert_eq!(find("Program file", "is valid"), Some(doctor::Level::Ok));
    assert_eq!(find("Engine", "not answering"), Some(doctor::Level::Fail));
    assert_eq!(find("Store", "no store"), Some(doctor::Level::Warn));
    assert!(!data.join("analyte.db").exists(), "doctor created a store");
    assert_eq!(
        find("Machine synthetic", "is free for Analyte"),
        Some(doctor::Level::Ok)
    );
    assert_eq!(
        find("Lab app demo", "gives no HTTPS answer"),
        Some(doctor::Level::Fail)
    );
    assert!(
        matches!(
            find("Lab app demo", "key"),
            Some(doctor::Level::Fail | doctor::Level::Warn)
        ),
        "{checks:?}"
    );
    assert_eq!(doctor::worst(&checks).exit_code(), 2);
    for check in &checks {
        assert!(!check.found.contains("whsec_"), "a key was printed");
    }
    Ok(())
}
