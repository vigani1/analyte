//! Lists and exports the recorded conversations kept under the data folder, each read audited first (ADR 0009, ADR 0034).
//!
//! A recording holds every byte the analyzer sent, so it **contains patient
//! data**. Listing and exporting are audited before anything is shown or
//! copied, as `parked show` is. Export copies one file, byte for byte, to a
//! folder the operator names and never overwrites a file there; it does not
//! de-identify anything. Sharing needs a reviewed de-identified copy
//! (docs/develop/adding-a-machine.md, "Capture, de-identify and replay").
use crate::installation::config::{Program, plain_name};
use crate::{deadline, operator::Error};
use analyte_model::{Identifier, Reason};
use analyte_store::Audit;
use clap::Subcommand;
use std::path::{Path, PathBuf};

/// Most recordings listed per machine; more are refused, never cut short.
const LIST_LIMIT: usize = 4096;

/// The file name every recording ends with (ADR 0009).
const SUFFIX: &str = ".rec.jsonl";

/// What every listing and export says first.
pub const PATIENT_DATA: &str = "These recordings contain patient data. Keep them on this PC; before sharing one, make a de-identified copy and review it (docs/develop/adding-a-machine.md, \"Capture, de-identify and replay\"). Analyte does not de-identify them.";

/// The `captures` commands.
#[derive(Subcommand)]
pub enum Command {
    /// Lists each machine's recordings: file, size and last change. They contain patient data.
    List {
        /// Only this machine's recordings.
        #[arg(long)]
        machine: Option<String>,
    },
    /// Copies one recording to a folder, byte for byte, without overwriting.
    /// It contains patient data: de-identify a copy before sharing it.
    Export {
        /// The machine's name.
        machine: String,
        /// The recording's file name, as `captures list` shows it.
        file: String,
        /// The folder to copy it into; it must exist.
        #[arg(long)]
        to: PathBuf,
        /// Why it is exported; the audit keeps it.
        #[arg(long)]
        reason: String,
    },
}

/// Why a recording cannot be listed or exported.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// The machine name could not name a capture folder.
    #[error("a machine name uses only letters, digits, - and _: {0}")]
    MachineName(String),
    /// The file name is not one recording in that folder.
    #[error(
        "name one recording as captures list shows it (a file name ending .rec.jsonl), not a path: {0}"
    )]
    FileName(String),
    /// No such recording.
    #[error("machine {machine} has no recording called {file}")]
    Missing {
        /// The machine.
        machine: String,
        /// The file asked for.
        file: String,
    },
    /// The target folder is not a folder.
    #[error("the folder {0} does not exist; create it first")]
    NoFolder(PathBuf),
    /// A file of that name is already in the target folder.
    #[error("{0} already exists; nothing was overwritten")]
    Exists(PathBuf),
    /// Too many recordings to list safely.
    #[error("machine {0} has more than 4096 recordings; move older ones out first")]
    TooMany(String),
    /// Reading or copying failed.
    #[error("{operation} {path} failed: {source}")]
    Io {
        /// What was done.
        operation: &'static str,
        /// On which file or folder.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

/// One recording file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// The machine whose folder holds it.
    pub machine: String,
    /// Its file name.
    pub file: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// When it last changed.
    pub modified: Option<jiff::Timestamp>,
    /// Capture stopped early and left a failure marker beside it.
    pub failed: bool,
}

fn io(operation: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> CaptureError {
    let path = path.to_owned();
    move |source| CaptureError::Io {
        operation,
        path,
        source,
    }
}

/// The capture folder of every machine, `<data>/captures/<machine>/`.
fn root(program: &Program) -> PathBuf {
    program.data_directory().join("captures")
}

fn machine_name(machine: &str) -> Result<(), CaptureError> {
    if machine.is_empty() || !plain_name(machine) {
        return Err(CaptureError::MachineName(machine.to_owned()));
    }
    Ok(())
}

/// The recordings of `machine`, sorted by file name; none when it has none.
/// # Errors
/// A bad machine name, too many files or a folder that cannot be read.
pub fn list(program: &Program, machine: &str) -> Result<Vec<Recording>, CaptureError> {
    machine_name(machine)?;
    let folder = root(program).join(machine);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io("read", &folder)(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io("read", &folder))?;
        let Some(file) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !file.ends_with(SUFFIX) || !entry.file_type().map_err(io("read", &folder))?.is_file() {
            continue;
        }
        if found.len() >= LIST_LIMIT {
            return Err(CaptureError::TooMany(machine.to_owned()));
        }
        let metadata = entry.metadata().map_err(io("read", &entry.path()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| jiff::Timestamp::try_from(time).ok());
        found.push(Recording {
            machine: machine.to_owned(),
            failed: folder.join(format!("{file}.failed")).exists(),
            file,
            bytes: metadata.len(),
            modified,
        });
    }
    found.sort_by(|left, right| left.file.cmp(&right.file));
    Ok(found)
}

/// Copies one recording into `to`, byte for byte, refusing to overwrite.
/// Returns the new file's path.
/// # Errors
/// A bad name, a missing recording or folder, an existing target or a copy failure.
pub fn export(
    program: &Program,
    machine: &str,
    file: &str,
    to: &Path,
) -> Result<PathBuf, CaptureError> {
    machine_name(machine)?;
    let plain = Path::new(file)
        .file_name()
        .is_some_and(|name| name == std::ffi::OsStr::new(file));
    if !plain || !file.ends_with(SUFFIX) || file.contains(['/', '\\']) {
        return Err(CaptureError::FileName(file.to_owned()));
    }
    let source = root(program).join(machine).join(file);
    if !source.is_file() {
        return Err(CaptureError::Missing {
            machine: machine.to_owned(),
            file: file.to_owned(),
        });
    }
    if !to.is_dir() {
        return Err(CaptureError::NoFolder(to.to_owned()));
    }
    let target = to.join(file);
    let mut output = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(CaptureError::Exists(target));
        }
        Err(error) => return Err(io("create", &target)(error)),
    };
    let mut input = std::fs::File::open(&source).map_err(io("open", &source))?;
    std::io::copy(&mut input, &mut output).map_err(io("copy to", &target))?;
    output.sync_all().map_err(io("flush", &target))?;
    Ok(target)
}

/// Runs one `captures` command; the audit is written before anything is shown or copied.
/// # Errors
/// Refuses bad names and reports audit, storage, file and output failures.
pub async fn run(program: &Program, actor: Identifier, command: Command) -> Result<(), Error> {
    let (action, targets, reason) = match &command {
        Command::List { machine } => (
            "captures-list",
            machine
                .iter()
                .map(|machine| Identifier::new(machine))
                .collect::<Result<Vec<_>, _>>()?,
            "operator listed recorded conversations".to_owned(),
        ),
        Command::Export {
            machine,
            file,
            reason,
            ..
        } => (
            "captures-export",
            vec![Identifier::new(machine)?, Identifier::new(file)?],
            reason.clone(),
        ),
    };
    let audit = Audit {
        actor,
        action: Identifier::new(action)?,
        targets,
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&reason)?,
    };
    let mut store = crate::open_system_store(program).await?;
    let audited = store
        .request(deadline()?, move |store| store.audit(&audit))
        .await;
    store.shutdown(deadline()?).await?;
    audited?;
    super::display::line(PATIENT_DATA)?;
    match command {
        Command::List { machine } => {
            let machines: Vec<String> = match machine {
                Some(machine) => vec![machine],
                None => program
                    .machines()
                    .iter()
                    .map(|machine| machine.id().as_str().to_owned())
                    .collect(),
            };
            for machine in machines {
                let recordings = list(program, &machine)?;
                if recordings.is_empty() {
                    super::display::line(&format!("{machine}: no recordings"))?;
                }
                for recording in recordings {
                    super::display::line(&format!(
                        "{} {} {} bytes, last changed {}{}",
                        recording.machine,
                        recording.file,
                        recording.bytes,
                        recording
                            .modified
                            .map_or_else(|| "unknown".to_owned(), |at| at.to_string()),
                        if recording.failed {
                            " (capture stopped early: incomplete)"
                        } else {
                            ""
                        }
                    ))?;
                }
            }
        }
        Command::Export {
            machine, file, to, ..
        } => {
            let target = export(program, &machine, &file, &to)?;
            super::display::line(&format!(
                "Copied to {}. It contains patient data: de-identify a copy before sharing it.",
                target.display()
            ))?;
        }
    }
    Ok(())
}
