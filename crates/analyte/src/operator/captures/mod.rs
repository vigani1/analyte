//! Lists, exports and de-identifies the recorded conversations kept under the data folder, each audited first (ADR 0009, ADR 0034, ADR 0037).
//!
//! A recording holds every byte the analyzer sent, so it **contains patient
//! data**. Every command is audited before anything is shown or copied, as
//! `parked show` is. Export copies one file, byte for byte, to a folder the
//! operator names and never overwrites a file there. De-identify writes a new
//! copy with synthetic patient fields and never changes the original; the
//! copy still needs a review before it is shared
//! (docs/develop/adding-a-machine.md, "Capture, de-identify and replay").
pub mod deidentify;

use crate::installation::config::{Program, plain_name};
use crate::{deadline, operator::Error};
use analyte_i18n::{message as tr_args, text as tr};
use analyte_model::{Identifier, Reason};
use analyte_store::Audit;
use clap::Subcommand;
use std::path::{Path, PathBuf};

/// Most recordings listed per machine; more are refused, never cut short.
const LIST_LIMIT: usize = 4096;

/// The file name every recording ends with (ADR 0009).
const SUFFIX: &str = ".rec.jsonl";

/// What every listing and export says first.
pub const PATIENT_DATA: &str = "These recordings contain patient data. Keep them on this PC; before sharing one, make a de-identified copy with captures de-identify and review it (docs/develop/adding-a-machine.md, \"Capture, de-identify and replay\").";

/// The `captures` commands.
#[derive(Subcommand)]
pub enum Command {
    /// Lists each machine's recordings: file, size and last change. They contain patient data.
    #[command(about = tr("cli-help-captures-command-list"))]
    List {
        /// Only this machine's recordings.
        #[arg(help = tr("cli-help-captures-command-list-machine"))]
        #[arg(long)]
        machine: Option<String>,
    },
    /// Copies one recording to a folder, byte for byte, without overwriting.
    /// It contains patient data: de-identify a copy before sharing it.
    #[command(about = tr("cli-help-captures-command-export"))]
    Export {
        /// The machine's name.
        #[arg(help = tr("cli-help-captures-command-export-machine"))]
        machine: String,
        /// The recording's file name, as `captures list` shows it.
        #[arg(help = tr("cli-help-captures-command-export-file"))]
        file: String,
        /// The folder to copy it into; it must exist.
        #[arg(help = tr("cli-help-captures-command-export-to"))]
        #[arg(long)]
        to: PathBuf,
        /// Why it is exported; the audit keeps it.
        #[arg(help = tr("cli-help-captures-command-export-reason"))]
        #[arg(long)]
        reason: String,
    },
    /// Writes a de-identified copy of one recording to a folder: patient
    /// fields become synthetic values of the same shape and checksums are
    /// recomputed. The original is not changed. Review the copy before sharing it.
    #[command(about = tr("cli-help-captures-command-de-identify"))]
    #[command(name = "de-identify")]
    DeIdentify {
        /// The machine's name.
        #[arg(help = tr("cli-help-captures-command-export-machine"))]
        machine: String,
        /// The recording's file name, as `captures list` shows it.
        #[arg(help = tr("cli-help-captures-command-export-file"))]
        file: String,
        /// The folder to write the copy into; it must exist.
        #[arg(help = tr("cli-help-captures-command-de-identify-to"))]
        #[arg(long)]
        to: PathBuf,
        /// Why it is de-identified; the audit keeps it.
        #[arg(help = tr("cli-help-captures-command-de-identify-reason"))]
        #[arg(long)]
        reason: String,
        /// The checksum letter case the machine uses (VIDAS); needed only when
        /// the recording cannot show it, and then the command says so.
        #[arg(help = tr("cli-help-captures-command-de-identify-checksum-case"))]
        #[arg(long, value_enum)]
        checksum_case: Option<deidentify::LetterCase>,
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
    /// The recording is not a readable ADR 0009 file.
    #[error("the recording cannot be read: {0}")]
    Format(#[from] analyte_transport::recording::FormatError),
    /// No de-identified copy could be made.
    #[error(transparent)]
    DeIdentify(#[from] deidentify::Error),
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
    let source = source(program, machine, file, to)?;
    let target = to.join(file);
    let mut output = create(&target)?;
    let mut input = std::fs::File::open(&source).map_err(io("open", &source))?;
    std::io::copy(&mut input, &mut output).map_err(io(tr("cli-copy-to"), &target))?;
    output.sync_all().map_err(io("flush", &target))?;
    Ok(target)
}

/// The stored recording `file` of `machine`, after checking both names and the target folder.
fn source(
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
    Ok(source)
}

/// A new file; an existing one is never replaced.
fn create(target: &Path) -> Result<std::fs::File, CaptureError> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
    {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(CaptureError::Exists(target.to_owned()))
        }
        Err(error) => Err(io("create", target)(error)),
    }
}

/// What a de-identification wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeIdentified {
    /// The new file.
    pub path: PathBuf,
    /// Values replaced, every occurrence counted.
    pub replaced: usize,
    /// Distinct real values replaced.
    pub distinct: usize,
}

/// Writes a de-identified copy of one recording into `to` as
/// `<name>.deidentified.rec.jsonl`, refusing to overwrite. The original is
/// only read, through the bounded ADR 0009 reader, and the whole copy is made
/// in memory first, so a refusal writes nothing.
/// # Errors
/// A bad name, a missing recording or folder, an unreadable recording, a
/// message whose patient fields cannot be placed, an existing target or a write failure.
pub fn de_identify(
    program: &Program,
    machine: &str,
    file: &str,
    to: &Path,
    case: Option<deidentify::LetterCase>,
) -> Result<DeIdentified, CaptureError> {
    use analyte_transport::recording::{self, Limits};
    let source = source(program, machine, file, to)?;
    let input = std::fs::File::open(&source).map_err(io("open", &source))?;
    let (header, events) = recording::read(std::io::BufReader::new(input), Limits::default())?;
    let copy = deidentify::de_identify(&header, &events, case)?;
    let mut encoded = Vec::new();
    recording::write_header(&mut encoded, &copy.header, Limits::default())
        .map_err(io("encode", &source))?;
    for event in &copy.events {
        recording::write_event(&mut encoded, event, Limits::default())
            .map_err(io("encode", &source))?;
    }
    if encoded.len() > Limits::default().file_bytes {
        return Err(io("encode", &source)(std::io::Error::other(tr(
            "cli-the-copy-exceeds-the-recording-file-bound",
        ))));
    }
    let stem = file.strip_suffix(SUFFIX).unwrap_or(file);
    let target = to.join(format!("{stem}.deidentified{SUFFIX}"));
    let mut output = create(&target)?;
    let written = std::io::Write::write_all(&mut output, &encoded)
        .and_then(|()| output.sync_all())
        .map_err(io("write", &target));
    if let Err(error) = written {
        drop(output);
        // A partial copy must not look like a finished one; the write error is what is reported.
        if let Err(cleanup) = std::fs::remove_file(&target) {
            tracing::warn!(error_kind = ?cleanup.kind(), "partial de-identified copy could not be removed");
        }
        return Err(error);
    }
    Ok(DeIdentified {
        path: target,
        replaced: copy.replaced,
        distinct: copy.distinct,
    })
}

/// Prints each machine's recordings, or that it has none.
fn show(program: &Program, machine: Option<String>) -> Result<(), Error> {
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
            super::display::line(&tr_args(
                "cli-no-recordings",
                [("machine", (machine).to_string().into())],
            ))?;
        }
        for recording in recordings {
            super::display::line(&tr_args(
                "cli-bytes-last-changed",
                [
                    ("machine", (recording.machine).to_string().into()),
                    ("file", (recording.file).to_string().into()),
                    ("bytes", (recording.bytes).to_string().into()),
                    (
                        "at",
                        (recording
                            .modified
                            .map_or_else(|| "unknown".to_owned(), |at| at.to_string()))
                        .to_string()
                        .into(),
                    ),
                    (
                        "else",
                        (if recording.failed {
                            " (capture stopped early: incomplete)"
                        } else {
                            ""
                        })
                        .to_string()
                        .into(),
                    ),
                ],
            ))?;
        }
    }
    Ok(())
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
        Command::DeIdentify {
            machine,
            file,
            reason,
            ..
        } => (
            "captures-de-identify",
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
        Command::List { machine } => show(program, machine)?,
        Command::Export {
            machine, file, to, ..
        } => {
            let target = export(program, &machine, &file, &to)?;
            super::display::line(&tr_args(
                "cli-copied-to-it-contains-patient-data-de-identify-a",
                [("target", (target.display()).to_string().into())],
            ))?;
        }
        Command::DeIdentify {
            machine,
            file,
            to,
            checksum_case,
            ..
        } => {
            let copy = de_identify(program, &machine, &file, &to, checksum_case)?;
            super::display::line(&tr_args(
                "cli-wrote-values-replaced-distinct-the-original-is-unchanged",
                [
                    ("path", (copy.path.display()).to_string().into()),
                    ("replaced", (copy.replaced).to_string().into()),
                    ("distinct", (copy.distinct).to_string().into()),
                ],
            ))?;
            super::display::line(deidentify::CHECKLIST)?;
        }
    }
    Ok(())
}
