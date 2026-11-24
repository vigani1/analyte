//! Builds, validates and writes a machine's site file, keeping the previous files.
//!
//! Validation runs first, in memory, through the same loader `check-config` uses.
//! Only then are files written: each old file is copied to `<name>.<time>.bak`, the
//! new text goes to `<name>.pending`, is flushed and renamed over the old one. The
//! whole program is then loaded again exactly as `check-config` does; if it fails,
//! the previous files are put back. Saving, adding and removing a machine all
//! write through [`commit`].
use super::SetupError;
use crate::installation::config::{
    Error as Installation, Program, folder, read, store_record_bytes, validate_machine,
};
use analyte_config::{Destination, Endpoint, LabApps, Validated, recording::SiteDocument};
use analyte_model::Identifier;
use std::{
    io::Write as _,
    ops::Range,
    path::{Path, PathBuf},
};

/// Reads a site's bounded TOML for setup, preserving the file and the cause of any error.
pub(super) fn read_document(path: &Path) -> Result<SiteDocument, SetupError> {
    let text = read(path).map_err(|source| SetupError::SiteRead {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    SiteDocument::parse(&path.display().to_string(), &text)
        .map_err(|source| SetupError::SiteFile(Box::new(source)))
}

/// One `[[machines]]` entry of the program file, with paths resolved.
pub(super) struct Installed {
    id: Identifier,
    capture: bool,
    /// Absent while the machine waits for setup.
    pub(super) definition: Option<PathBuf>,
    placement: Placement,
    pub(super) site: PathBuf,
    overrides: Option<PathBuf>,
}

/// Where a chosen definition goes in the program file's text.
enum Placement {
    /// The bytes of the current `definition` value.
    Replace(Range<usize>),
    /// The start of the `site` line, for an entry still waiting for setup.
    Insert(usize),
}

#[derive(serde::Deserialize)]
pub(super) struct ProgramSpans {
    #[serde(default)]
    pub(super) machines: Vec<EntrySpans>,
}

/// One `[[machines]]` entry as written, with the byte positions of its values.
#[derive(serde::Deserialize)]
pub(super) struct EntrySpans {
    pub(super) id: toml::Spanned<String>,
    pub(super) definition: Option<toml::Spanned<String>>,
    pub(super) site: toml::Spanned<String>,
    overrides: Option<String>,
    #[serde(default)]
    capture: bool,
}

/// Reads the program file and its entries as written.
pub(super) fn entries(config: &Path) -> Result<(String, Vec<EntrySpans>), SetupError> {
    let text = read(config)?;
    let spans: ProgramSpans = toml::from_str(&text).map_err(SetupError::ProgramFile)?;
    Ok((text, spans.machines))
}

/// Reads the program file and finds the entry for machine `id`.
pub(super) fn installed(config: &Path, id: &str) -> Result<(String, Installed), SetupError> {
    let (text, listed) = entries(config)?;
    let base = folder(config);
    let entry = listed
        .into_iter()
        .find(|entry| entry.id.get_ref() == id)
        .ok_or_else(|| SetupError::UnknownMachine(id.into()))?;
    let placement = match &entry.definition {
        Some(definition) => Placement::Replace(definition.span()),
        None => Placement::Insert(site_line(&text, entry.site.span().start)?),
    };
    let installed = Installed {
        id: Identifier::new(entry.id.get_ref()).map_err(SetupError::InvalidName)?,
        capture: entry.capture,
        definition: entry.definition.map(|d| base.join(d.get_ref())),
        placement,
        site: base.join(entry.site.get_ref()),
        overrides: entry.overrides.map(|path| base.join(path)),
    };
    Ok((text, installed))
}

impl Installed {
    /// Keeps the existing path when both paths lead to the same file. Model
    /// identity depends on the path left in the program file (ADR 0033).
    pub(super) fn definition_to_save(&self, proposed: PathBuf) -> PathBuf {
        match &self.definition {
            Some(current) if same_file(current, &proposed) => current.clone(),
            _ => proposed,
        }
    }
}

/// The start of the line holding the `site` value at `value`; the definition of
/// a waiting machine is inserted there, so it must be a plain `site = ...` line.
fn site_line(text: &str, value: usize) -> Result<usize, SetupError> {
    let start = text
        .get(..value)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline.saturating_add(1));
    match text.get(start..value) {
        Some(line) if line.trim_start().starts_with("site") => Ok(start),
        _ => Err(SetupError::ProgramLayout(
            "write this machine's site = ... on its own line so setup can add the definition",
        )),
    }
}

/// The site-file line a connection becomes, for display.
pub(super) fn describe(connection: &Endpoint) -> String {
    match connection {
        Endpoint::UsbSerial(serial) => format!("usb_serial = {}", serial.as_str()),
        Endpoint::Fixed(endpoint) => format!("endpoint = {endpoint}"),
    }
}

/// A site file with the page's choices applied, and the lab apps it needs.
pub(super) struct Candidate {
    /// The new site file text.
    pub(super) text: String,
    /// The lab apps after the save; written only when they differ from the current ones.
    pub(super) lab_apps: LabApps,
}

/// The current site file with the page's choices applied: the connection, the
/// devices it accepts (`allowed_peers`, removed when none are chosen, ADR
/// 0026), the serial cable settings (`[serial]`, every chosen value, removed
/// for other links, ADR 0031), the model's own choices (`[choices]`, every
/// declared choice's value, removed for a model without choices, ADR 0032),
/// and `lab_app = "<name>"` in place of any `[destination]` table. Every other key (retention, time zone, limits) is
/// kept as it was.
///
/// A complete `[destination]` moves into `lab-apps.toml` unless it is already
/// there with identical values, so results waiting for it stay deliverable even
/// when another lab app is picked. A partial one (the installers' starter key
/// names, which never loaded) is dropped. A picked lab app written only
/// elsewhere (another machine's site file, or the program file by an older
/// setup page) is copied into `lab-apps.toml` with the same values.
pub(super) fn candidate(
    mut table: toml::Table,
    (connection, peers, serial, chosen): (
        &Endpoint,
        Option<&analyte_config::AllowedPeers>,
        Option<&analyte_config::SerialValues>,
        &analyte_config::ChoiceSelections,
    ),
    (autolyser_poll_seconds, vidas_patient_namespace): (Option<u32>, Option<&str>),
    lab_app: &Identifier,
    lab_apps: &LabApps,
    elsewhere: &[Destination],
) -> Result<Candidate, SetupError> {
    apply_link(&mut table, (connection, peers, serial, chosen))?;
    table.remove("autolyser_poll_seconds");
    table.remove("vidas_patient_namespace");
    if let Some(seconds) = autolyser_poll_seconds {
        table.insert(
            "autolyser_poll_seconds".into(),
            toml::Value::Integer(i64::from(seconds)),
        );
    }
    if let Some(namespace) = vidas_patient_namespace {
        table.insert(
            "vidas_patient_namespace".into(),
            toml::Value::String(namespace.into()),
        );
    }
    let mut after = keep_own_destination(&mut table, lab_apps)?;
    if after.get(lab_app.as_str()).is_none() {
        let old = elsewhere
            .iter()
            .find(|known| &known.name == lab_app)
            .ok_or_else(|| SetupError::UnknownLabApp(lab_app.as_str().into()))?;
        after = moved(&after, old.clone())?;
    }
    table.remove("lab_app");
    table.insert(
        "lab_app".into(),
        toml::Value::String(lab_app.as_str().into()),
    );
    let text = toml::to_string(&table).map_err(SetupError::SiteText)?;
    Ok(Candidate {
        text: format!("# Written by the Analyte setup page.\n{text}"),
        lab_apps: after,
    })
}

/// Writes the connection, the devices it accepts (`allowed_peers`, removed
/// when none are chosen, ADR 0026), the model's own choices (`[choices]`,
/// removed when there are none, ADR 0032) and the serial cable settings
/// (`[serial]`, removed for other links, ADR 0031) into a site table, and
/// leaves record only (`[recording]` goes, ADR 0034).
fn apply_link(
    table: &mut toml::Table,
    (connection, peers, serial, chosen): (
        &Endpoint,
        Option<&analyte_config::AllowedPeers>,
        Option<&analyte_config::SerialValues>,
        &analyte_config::ChoiceSelections,
    ),
) -> Result<(), SetupError> {
    table.remove("endpoint");
    table.remove("usb_serial");
    let (key, value) = match connection {
        Endpoint::UsbSerial(serial) => ("usb_serial", serial.as_str()),
        Endpoint::Fixed(endpoint) => ("endpoint", endpoint.as_str()),
    };
    table.insert(key.into(), toml::Value::String(value.into()));
    table.remove("allowed_peers");
    if let Some(peers) = peers {
        table.insert(
            "allowed_peers".into(),
            toml::Value::Array(
                peers
                    .addresses()
                    .iter()
                    .map(|address| toml::Value::String(address.to_string()))
                    .collect(),
            ),
        );
    }
    // A normal connection leaves record only (ADR 0034); its recorded
    // messages stay kept and are never delivered.
    table.remove("recording");
    table.remove("choices");
    if !chosen.is_empty() {
        table.insert(
            "choices".into(),
            toml::Value::try_from(chosen).map_err(SetupError::SiteText)?,
        );
    }
    table.remove("serial");
    if let Some(serial) =
        serial.filter(|serial| **serial != analyte_config::SerialValues::default())
    {
        table.insert(
            "serial".into(),
            toml::Value::try_from(serial).map_err(SetupError::SiteText)?,
        );
    }
    Ok(())
}

/// The current site file made record-only (ADR 0034): the connection, the
/// devices it accepts and the cable as [`candidate`] writes them, the
/// capture profile's answers in `[recording]`, and no lab app. A complete
/// `[destination]` moves into `lab-apps.toml`, as [`candidate`] does, so
/// results already waiting for it stay deliverable; `lab_app` and the
/// model's `[choices]` go.
pub(super) fn recording_candidate(
    mut table: toml::Table,
    (connection, peers, serial): (
        &Endpoint,
        Option<&analyte_config::AllowedPeers>,
        Option<&analyte_config::SerialValues>,
    ),
    answers: &analyte_config::ProfileAnswers,
    lab_apps: &LabApps,
) -> Result<Candidate, SetupError> {
    let none = analyte_config::ChoiceSelections::default();
    table.remove("autolyser_poll_seconds");
    table.remove("vidas_patient_namespace");
    let after = keep_own_destination(&mut table, lab_apps)?;
    table.remove("lab_app");
    apply_link(&mut table, (connection, peers, serial, &none))?;
    table.insert(
        "recording".into(),
        toml::Value::try_from(answers).map_err(SetupError::SiteText)?,
    );
    let text = toml::to_string(&table).map_err(SetupError::SiteText)?;
    Ok(Candidate {
        text: format!("# Written by the Analyte setup page.\n{text}"),
        lab_apps: after,
    })
}

/// Takes the site table's own `[destination]` out and returns `lab_apps` with
/// it added, so results waiting for it stay deliverable; a partial one (the
/// installers' starter key names, which never loaded) is dropped.
fn keep_own_destination(
    table: &mut toml::Table,
    lab_apps: &LabApps,
) -> Result<LabApps, SetupError> {
    let Some(own) = table.remove("destination") else {
        return Ok(lab_apps.clone());
    };
    let complete = own.get("name").is_some() || own.get("base_url").is_some();
    match own.try_into::<Destination>() {
        Ok(own) => moved(lab_apps, own),
        Err(_) if !complete => Ok(lab_apps.clone()),
        Err(error) => Err(SetupError::SiteDestination(error)),
    }
}

/// `lab_apps` with `lab_app` added; unchanged when it is already listed with
/// identical values. Other values under the same name are refused, never chosen.
pub(super) fn moved(lab_apps: &LabApps, lab_app: Destination) -> Result<LabApps, SetupError> {
    match lab_apps.get(lab_app.name.as_str()) {
        Some(listed) if *listed == lab_app => Ok(lab_apps.clone()),
        Some(_) => Err(SetupError::LabAppDiffers(lab_app.name.as_str().into())),
        None => Ok(lab_apps.with(lab_app)?),
    }
}

/// Validates `definition` with `site_text`, the installed overrides and
/// `lab_apps`, exactly as the program loader does for one machine. Writes nothing.
pub(super) fn validate(
    program: &str,
    installed: &Installed,
    definition: &Path,
    site_text: &str,
    lab_apps: &LabApps,
) -> Result<Validated, SetupError> {
    let checked = validate_machine(
        (&installed.id, installed.capture),
        definition,
        (&installed.site, Some(site_text)),
        installed.overrides.as_deref(),
        store_record_bytes(program)?,
        lab_apps,
    )
    .map_err(|error| {
        if let Installation::Config(error) = error {
            SetupError::Config(error)
        } else {
            SetupError::Installation(error)
        }
    })?;
    Ok(checked.snapshot)
}

/// The program text with this entry's `definition` replaced, or added on its own
/// line before `site` for a waiting machine; comments and every other byte stay
/// as they were. A definition inside the program file's folder is
/// written relative to it, with `/`, as the shipped examples are.
fn with_definition(
    program: &str,
    config: &Path,
    installed: &Installed,
    definition: &Path,
) -> Result<String, SetupError> {
    let relative = config
        .parent()
        .and_then(|folder| definition.strip_prefix(folder).ok())
        .and_then(|path| {
            path.components()
                .map(|part| match part {
                    std::path::Component::Normal(name) => name.to_str(),
                    std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::CurDir
                    | std::path::Component::ParentDir => None,
                })
                .collect::<Option<Vec<_>>>()
        })
        .map(|parts| parts.join("/"));
    let text = relative.unwrap_or_else(|| definition.display().to_string());
    let value = toml::Value::String(text).to_string();
    let (range, line) = match &installed.placement {
        Placement::Replace(range) => (range.clone(), false),
        Placement::Insert(at) => (*at..*at, true),
    };
    let (Some(before), Some(after)) = (program.get(..range.start), program.get(range.end..)) else {
        return Err(SetupError::ProgramLayout("definition position changed"));
    };
    Ok(if line {
        format!("{before}definition = {value}\n{after}")
    } else {
        format!("{before}{value}{after}")
    })
}

/// Files written by a save.
pub(super) struct Written {
    pub(super) backups: Vec<PathBuf>,
}

/// Writes the site file, `lab-apps.toml` when `lab_apps` is given, and the
/// program entry's definition when it changed, keeping backups; puts the old
/// files back if the program no longer loads.
pub(super) fn apply(
    config: &Path,
    program: &str,
    installed: &Installed,
    definition: &Path,
    site_text: &str,
    lab_apps: Option<String>,
) -> Result<Written, SetupError> {
    let mut changes = vec![(installed.site.clone(), site_text.to_owned())];
    if let Some(text) = lab_apps {
        changes.push((crate::installation::config::lab_apps::path(config), text));
    }
    if installed.definition.as_deref() != Some(definition) {
        changes.push((
            config.to_owned(),
            with_definition(program, config, installed, definition)?,
        ));
    }
    commit(config, &changes)
}

/// The time label of backups made now.
fn label() -> String {
    jiff::Timestamp::now()
        .strftime("%Y%m%dT%H%M%SZ")
        .to_string()
}

/// Writes each `(path, text)`, keeping a dated backup of every file that existed,
/// then loads the whole program as `check-config` does; if writing or loading
/// fails, every file is put back as it was.
pub(super) fn commit(config: &Path, changes: &[(PathBuf, String)]) -> Result<Written, SetupError> {
    let label = label();
    let mut backups = Vec::new();
    for (path, _) in changes {
        backups.push(backup(path, &label)?);
    }
    for (path, text) in changes {
        if let Err(error) = replace(path, text.as_bytes()) {
            restore(changes, &backups);
            return Err(error);
        }
    }
    if let Err(error) = Program::load(config) {
        restore(changes, &backups);
        return Err(SetupError::RolledBack(error));
    }
    Ok(Written {
        backups: backups.into_iter().flatten().collect(),
    })
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn io(operation: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> SetupError {
    let path = path.to_owned();
    move |source| SetupError::Io {
        operation,
        path,
        source,
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    path.with_file_name(name)
}

/// Copies the current file to `<name>.<label>.bak` (or `<name>.<label>-<n>.bak`
/// when saved twice in one second); a missing file needs none. Never overwrites.
fn backup(path: &Path, label: &str) -> Result<Option<PathBuf>, SetupError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(io("read for backup", path))?;
    let mut attempt = 0_u8;
    let (target, mut file) = loop {
        let suffix = if attempt == 0 {
            format!(".{label}.bak")
        } else {
            format!(".{label}-{attempt}.bak")
        };
        let target = sibling(path, &suffix);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => break (target, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 99 => {
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(io("create backup", &target)(error)),
        }
    };
    file.write_all(&bytes)
        .map_err(io("write backup", &target))?;
    file.sync_all().map_err(io("flush backup", &target))?;
    Ok(Some(target))
}

/// Writes `<name>.pending`, flushes it and renames it over `path`.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let pending = sibling(path, ".pending");
    let mut file = std::fs::File::create(&pending).map_err(io("create", &pending))?;
    file.write_all(bytes).map_err(io("write", &pending))?;
    file.sync_all().map_err(io("flush", &pending))?;
    drop(file);
    std::fs::rename(&pending, path).map_err(io("replace", path))
}

/// Moves a file the program no longer names to `<name>.<time>.bak`: a durable
/// copy first, then the original goes. A missing file needs nothing.
pub(super) fn retire(path: &Path) -> Result<Option<PathBuf>, SetupError> {
    let Some(kept) = backup(path, &label())? else {
        return Ok(None);
    };
    std::fs::remove_file(path).map_err(io("move to the backup", path))?;
    Ok(Some(kept))
}

/// Puts each backup back over its file. A file that did not exist before (it has
/// no backup) was written only by this change, so it is removed again.
fn restore(changes: &[(PathBuf, String)], backups: &[Option<PathBuf>]) {
    for ((path, _), backup) in changes.iter().zip(backups) {
        let Some(backup) = backup else {
            // Not found: the write failed before this file was reached.
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::error!(%error, "setup could not remove a file it had just written");
            }
            continue;
        };
        match std::fs::read(backup) {
            Ok(bytes) => {
                if let Err(error) = replace(path, &bytes) {
                    tracing::error!(%error, "setup could not restore a previous file");
                }
            }
            Err(error) => {
                tracing::error!(%error, "setup could not read a backup to restore it");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Program, SetupError};
    use crate::installation::config::Error as Installation;
    use analyte_fixtures::ConfigEdit as _;

    const ASTM: &str = include_str!("../../../../core/config/examples/astm.toml");
    const HL7: &str = include_str!("../../../../core/config/examples/hl7.toml");
    const SITE: &str = include_str!("../../../../core/config/examples/site.toml");

    fn assert_refused(
        (definition, definition_text): (&str, &str),
        (id, capture): (&str, bool),
        site_text: &str,
        refuses: fn(&Installation) -> bool,
    ) -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        let config = root.join("analyte.toml");
        let program = analyte_fixtures::program(&format!(
            "[[machines]]\nid = '{id}'\ncapture = {capture}\ndefinition = '{definition}'\nsite = 'site.toml'\n"
        ));
        std::fs::write(&config, &program)?;
        let definition = root.join(definition);
        std::fs::create_dir_all(definition.parent().ok_or("definition folder")?)?;
        std::fs::write(&definition, definition_text)?;
        let site = root.join("site.toml");
        let original = format!("# Original settings\n{SITE}");
        std::fs::write(&site, &original)?;
        let (program, installed) = super::installed(&config, id)?;
        let result = super::validate(
            &program,
            &installed,
            &definition,
            site_text,
            &analyte_config::LabApps::none("lab-apps.toml"),
        );
        let error = match result {
            Err(SetupError::Installation(error)) => error,
            Err(other) => return Err(other.into()),
            Ok(_) => return Err("setup accepted the input".into()),
        };
        assert!(refuses(&error), "{error}");
        assert_eq!(std::fs::read_to_string(&config)?, program);
        assert_eq!(std::fs::read_to_string(&site)?, original);
        assert_eq!(std::fs::read_dir(root)?.count(), 3);

        std::fs::write(&site, site_text)?;
        let startup = Program::load(&config)
            .err()
            .ok_or("startup accepted the input")?;
        assert!(refuses(&startup), "{startup}");
        assert_eq!(error.to_string(), startup.to_string());
        Ok(())
    }

    // AGENTS.md "One way per thing": Setup and startup enforce the same checksum policy.
    #[test]
    fn setup_refuses_lowercase_astm_checksums() -> Result<(), crate::Error> {
        let declaration = include_str!("../../../../core/config/tests/fixtures/departure.toml")
            .rewrite("astm_frame_limit", "astm_lowercase_checksum_hex");
        let definition = ASTM.rewrite("deviations = []", &declaration).rewrite(
            "checksum_hex = \"strict\"",
            "checksum_hex = \"accept_lowercase\"",
        );
        assert_refused(
            (analyte_fixtures::DEFINITION, &definition),
            ("lab-one", false),
            &SITE.rewrite("127.0.0.1:5600", "COM3"),
            |error| matches!(error, Installation::UnsupportedChecksumMode),
        )
    }

    // AGENTS.md "One way per thing": Setup and startup enforce the same operation deadline.
    #[test]
    fn setup_refuses_operation_timeouts_over_sixty_seconds() -> Result<(), crate::Error> {
        let definition = HL7.rewrite(
            "operation_timeout_ms = 10000",
            "operation_timeout_ms = 60001",
        );
        assert_refused(
            (analyte_fixtures::DEFINITION, &definition),
            ("lab-one", false),
            SITE,
            |error| matches!(error, Installation::ReceiveTimeoutTooLong),
        )
    }

    // AGENTS.md "One way per thing": Setup and startup enforce the same store capacity.
    #[test]
    fn setup_refuses_machine_data_larger_than_the_store_record_limit() -> Result<(), crate::Error> {
        let definition = HL7.rewrite("bytes = 1048576", "bytes = 4194304");
        assert_refused(
            (analyte_fixtures::DEFINITION, &definition),
            ("lab-one", false),
            SITE,
            |error| matches!(error, Installation::StoreRecordLimitTooSmall),
        )
    }

    // AGENTS.md "One way per thing" and ADR 0034: both ways of capturing need a plain name.
    #[test]
    fn setup_refuses_non_plain_capture_names() -> Result<(), crate::Error> {
        let refuses = |error: &Installation| matches!(error, Installation::InvalidCaptureMachineId);
        assert_refused(
            (analyte_fixtures::DEFINITION, HL7),
            ("lab.one", true),
            SITE,
            refuses,
        )?;
        assert_refused(
            (
                "test/synthetic/record.toml",
                "recording_schema_version = 1\nprotocol = 'hl7'\n[transport]\nkind = 'tcp_server'\n",
            ),
            ("lab.one", false),
            "schema_version = 1\nretention = 'keep'\nendpoint = '127.0.0.1:5600'\n[recording]\n",
            refuses,
        )
    }

    // ADR 0033: the definition's folders must supply a valid model key.
    #[test]
    fn setup_refuses_definitions_outside_a_valid_model_folder() -> Result<(), crate::Error> {
        assert_refused(
            ("test/invalid_model/machine.toml", HL7),
            ("lab-one", false),
            SITE,
            |error| matches!(error, Installation::DefinitionOutsideModelFolder { .. }),
        )
    }

    // Owner request 2026-11-18 (setup page): changing the definition edits only
    // that value in the program file and keeps comments.
    #[test]
    fn definition_edit_keeps_the_rest_of_the_program_file() -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("analyte.toml");
        let program = "# keep me\nschema_version = 1\n[[machines]]\nid = 'a'\ndefinition = 'old.toml' # old\nsite = 's.toml'\n";
        std::fs::write(&config, program)?;
        let (text, installed) = super::installed(&config, "a")?;
        let target = std::path::Path::new(r"C:\Program Files\Analyte\machines\x\y\machine.toml");
        let edited = super::with_definition(&text, &config, &installed, target)?;
        assert!(edited.starts_with("# keep me\n"));
        assert!(edited.ends_with(" # old\nsite = 's.toml'\n"));
        let parsed: toml::Table = edited.parse()?;
        let definition = parsed
            .get("machines")
            .and_then(|m| m.get(0))
            .and_then(|m| m.get("definition"))
            .and_then(toml::Value::as_str);
        assert_eq!(definition, Some(target.to_str().ok_or("path")?));
        let inside = directory
            .path()
            .join("machines")
            .join("x")
            .join("machine.toml");
        let edited = super::with_definition(&text, &config, &installed, &inside)?;
        assert!(
            edited.contains("definition = \"machines/x/machine.toml\" # old"),
            "{edited}"
        );
        Ok(())
    }

    // Owner request 2026-11-18 (installers): a machine waiting for setup gets its
    // definition as a new line before `site`; nothing else changes.
    #[test]
    fn waiting_machine_gets_a_definition_line() -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("analyte.toml");
        let program = "schema_version = 1\n[[machines]]\nid = 'a'\n  site = 's.toml' # s\n";
        std::fs::write(&config, program)?;
        let (text, installed) = super::installed(&config, "a")?;
        assert!(installed.definition.is_none());
        let inside = directory
            .path()
            .join("machines")
            .join("x")
            .join("machine.toml");
        let edited = super::with_definition(&text, &config, &installed, &inside)?;
        assert_eq!(
            edited,
            "schema_version = 1\n[[machines]]\nid = 'a'\ndefinition = \"machines/x/machine.toml\"\n  site = 's.toml' # s\n"
        );
        // An inline entry has no `site` line to insert before; refuse rather than guess.
        std::fs::write(&config, "machines = [{ id = 'a', site = 's.toml' }]\n")?;
        assert!(super::installed(&config, "a").is_err());
        Ok(())
    }
}
