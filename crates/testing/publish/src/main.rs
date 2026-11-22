//! The `analyte-publish` command: writes the published schema, OpenAPI and catalog files into docs/contract/, or with `--check` reports the stale ones.
#![forbid(unsafe_code)]
use std::{io::Write as _, process::ExitCode};

fn main() -> ExitCode {
    let check = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(_) => {
            say("usage: analyte-publish [--check]");
            return ExitCode::from(2);
        }
    };
    let repository = analyte_publish::repository();
    let files = match analyte_publish::files(&repository.join("machines")) {
        Ok(files) => files,
        Err(error) => {
            say(&format!("analyte-publish: {error}"));
            return ExitCode::from(2);
        }
    };
    let folder = repository.join(analyte_publish::FOLDER);
    let mut stale = false;
    for file in files {
        let path = folder.join(file.name);
        let current = std::fs::read_to_string(&path).ok();
        if current.as_deref() == Some(file.text.as_str()) {
            continue;
        }
        if check {
            say(&format!(
                "{}/{} is stale; run `cargo run -p analyte-publish`",
                analyte_publish::FOLDER,
                file.name
            ));
            stale = true;
        } else if let Err(error) = std::fs::write(&path, file.text) {
            say(&format!("analyte-publish: {}: {error}", path.display()));
            return ExitCode::from(2);
        }
    }
    if stale {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Writes one line to standard error; a closed stream is not worth a panic.
fn say(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}
