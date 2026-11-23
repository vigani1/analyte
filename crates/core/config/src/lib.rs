//! Loads bounded configuration and validates settings and local evidence without connection I/O.
#![forbid(unsafe_code)]

mod endpoint;
pub use endpoint::{Endpoint, NotFixed, UsbSerial};
mod https;
pub use https::{HttpsUrl, as_base, loopback_test_url};
mod lab_apps;
mod peers;
pub use lab_apps::{LAB_APPS_FILE, LabAppProblem, LabAppSource, LabApps, MAX_LAB_APPS};
pub use peers::AllowedPeers;
mod orders;
mod retention;
pub use retention::Retention;
mod time_zone;
pub use time_zone::SiteTimeZone;
mod selection;
use selection::ParsedMachine;
pub use selection::StackChoice;
mod schema;
use schema::MachineDocument;
mod choices;
pub use choices::{Chosen, declared as declared_choices};
pub mod recording;
pub use recording::{
    AnalyzerMode, AnalyzerModeValue, ProfileAnswers, RECORD_FILE, RecordLink, RecordStack,
    VidasAcknowledgment,
};
mod stack;
mod validation;
pub use orders::{
    AstmOrderFields, AstmOrderProfile, DsrLine, Hl7OrderQuery, Hl7Processing, Hl7QueryFilter,
    Hl7Send, MachinePriority, OrderPriority, OrderQuery, Orders, PriorityCode, WhenUntrusted,
};
pub use schema::software_version;
pub use schema::{
    AckPoint, AstmResultLayout, AstmResultProfile, AstmTestComponent, Autolyser, Capability,
    ChecksumAcceptance, ChecksumHex, ChecksumInclusion, ChoiceBlock, ChoiceKind, ChoiceLabel,
    ChoiceName, ChoiceSelections, ChoiceValue, Classification, CodeTables, ComponentParts,
    DepartureDeclaration, DepartureDirection, DepartureEdition, DepartureGuarantee,
    DepartureMessage, DepartureReference, DepartureRefusal, DepartureScope, Destination, Dialog,
    DilutionSource, Edition, Encoding, Escapes, Evidence, EvidenceLevel, Field, FlowControl,
    Framing, Group, HexCase, IdentityEvidence, IdentityReference, Law, LawConnectionMode, LawPeer,
    LawQueryForm, LawWork, MAX_CHOICE_COMBINATIONS, MAX_CHOICE_VALUES, MAX_CHOICES, MachineChoice,
    MachineDeparture, Mappings, NoSubId, OrderNamespace, OrdersSwitch, Overrides, Parity,
    PeerSupport, Poct, PoctIdentity, Priority, QcRule, Responsibility, SampleSelection,
    SampleSource, Serial, SerialSetting, SerialValues, Site, SubId, Syntax, Terminator, Transport,
    TransportKind, UnitMapping, ValueExpectation, ValueKind, Vidas, VidasChoice, VidasIdentity,
    VidasResultControlLabels, VidasSampleLimit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use stack::{
    AstmOrders, AstmStack, AstmSyntax, AutolyserOrders, AutolyserStack, DialogTimeouts,
    FileAstmStack, FileHl7Stack, Hl7OptionsSource, Hl7Orders, Hl7Query, Hl7SendOrders, Hl7Stack,
    Hl7Syntax, LawLinks, LawOrders, LawPolicy, LawStack, OrderTables, PoctStack, RecordTransport,
    Stack, StackSettings, StreamKind, StreamTransport, SyntaxBounds, SyntaxLimits, TcpRole,
    TcpTransport, VidasOrders, VidasStack,
};
use validation::{validate_machine, validate_site};
#[cfg(test)]
mod tests;

/// Maximum bytes accepted per TOML layer, before parsing or allocation.
pub const MAX_FILE_BYTES: usize = 1_048_576;

/// Lists the HL7 editions with result-processing rules, from the edition table.
fn hl7_result_editions() -> String {
    analyte_hl7::Version::result_editions()
        .map(analyte_hl7::Version::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A config failure names its source file and actionable key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A named machine departure failed a specific activation prerequisite.
    #[error("{path}: departure {rule}: {reason}")]
    Departure {
        /// Configuration source.
        path: String,
        /// Stable rule tag or migration key.
        rule: String,
        /// Exact refusal, without payload contents.
        reason: DepartureRefusal,
        /// Original local I/O failure when reference resolution or reading failed.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    /// The original query profile was withdrawn in the configured HL7 edition.
    #[error(
        "{path}: orders.hl7: HL7 {edition} withdraws the original query messages (§{section}); legacy query is not available for this edition"
    )]
    WithdrawnHl7Query {
        /// Source configuration layer.
        path: String,
        /// Exact requested edition.
        edition: &'static str,
        /// Withdrawing chapter section in that edition.
        section: &'static str,
    },
    /// A recognized HL7 edition has no implemented result-processing profile.
    #[error(
        "{path}: syntax.edition: HL7 {edition} is recognized but not yet supported for result processing; supported: {}",
        hl7_result_editions()
    )]
    UnsupportedHl7Edition {
        /// Source configuration layer.
        path: String,
        /// Exact recognized wire edition, without a patch-edition upgrade.
        edition: &'static str,
    },
    /// A part separator collides with a protocol delimiter.
    #[error("{path}: component_parts separator is a protocol delimiter")]
    PartSeparator {
        /// Source layer.
        path: String,
    },
    /// A component or part position is outside the declared bounds.
    #[error("{path}: component_parts position is out of range")]
    PartIndex {
        /// Source layer.
        path: String,
    },
    /// Code and dilution cannot select the same part.
    #[error("{path}: component_parts code and dilution must differ")]
    PartOverlap {
        /// Source layer.
        path: String,
    },
    /// Required identity configuration is absent; no mapping default is invented.
    #[error("{path}: mappings.identity.{setting}: required setting is missing")]
    MissingIdentitySetting {
        /// Source TOML layer.
        path: String,
        /// Missing identity setting.
        setting: &'static str,
    },
    /// Protocol layers or transport cannot be combined.
    #[error("{path}: incompatible protocol stack: {reason}")]
    IncompatibleStack {
        /// Required compatible settings.
        reason: &'static str,
        /// Source layer.
        path: String,
    },
    /// Field selectors cannot interpret vendor messages.
    #[error("{path}: vendor syntax requires codes, without field-position mappings")]
    VendorFieldMappings {
        /// Source layer.
        path: String,
    },
    /// A required installed policy is missing, invalid or belongs to another stack.
    #[error("{path}: invalid or missing {protocol} settings")]
    ProtocolSettings {
        /// Source layer.
        path: String,
        /// Policy needing correction.
        protocol: &'static str,
    },
    /// Input exceeds the file budget.
    #[error("{path}: file exceeds 1048576 bytes; split or reduce the configuration")]
    FileSize {
        /// Source path.
        path: String,
    },
    /// TOML syntax or schema error, including unknown keys.
    #[error("{path}: {source}")]
    Schema {
        /// Source path.
        path: String,
        /// Original positioned TOML diagnostic.
        source: toml::de::Error,
    },
    /// A known key has an invalid value or layer combination.
    #[error("{path}: {key}: {reason}")]
    Invalid {
        /// Source file or effective snapshot.
        path: String,
        /// Qualified key.
        key: String,
        /// Required corrective action.
        reason: &'static str,
    },
    /// A record-only site file does not say which mode the analyzer's own
    /// settings screen shows, though the model's record file asks it; none is
    /// assumed (ADR 0034).
    #[error(
        "{path}: recording.analyzer_mode: no value: {question}? Choose one of {allowed} on the Setup page (step 2) or with analyte machines set --analyzer-mode <value>"
    )]
    MissingAnalyzerMode {
        /// Site file.
        path: String,
        /// The model's question, in plain words.
        question: String,
        /// The values the model offers.
        allowed: String,
    },
    /// A record-only site file names an analyzer mode the model does not offer.
    #[error("{path}: recording.analyzer_mode: {value} is not one of the model's values: {allowed}")]
    AnalyzerModeValue {
        /// Site file.
        path: String,
        /// The refused value.
        value: ChoiceName,
        /// The values the model offers.
        allowed: String,
    },
    /// A record-only site file leaves a question of its protocol's capture
    /// profile unanswered; no answer is assumed (ADR 0034).
    #[error(
        "{path}: recording.{key}: no value: {question}? Answer it on the Setup page (step 2) or with analyte machines set"
    )]
    MissingRecordingAnswer {
        /// Site file.
        path: String,
        /// The `[recording]` key.
        key: &'static str,
        /// The question, in plain words.
        question: &'static str,
    },
    /// A serial cable setting has no value in any layer; none is assumed (ADR 0031).
    #[error(
        "{path}: transport.serial.{setting}: no value: the machine definition suggests none and none was chosen; choose the analyzer's own setting on the Setup page (step 2) or with analyte machines set"
    )]
    MissingSerialSetting {
        /// The layer whose stack lacks the value.
        path: String,
        /// The setting without a value.
        setting: SerialSetting,
    },
    /// A choice the definition declares has no value in any layer; none is
    /// assumed (ADR 0032).
    #[error(
        "{path}: choices.{choice}: no value: the machine definition suggests none and none was chosen; choose one of {allowed} on the Setup page (step 2) or with analyte machines set --choice {choice}=<value>"
    )]
    MissingChoice {
        /// The site file that lacks the selection.
        path: String,
        /// The choice without a value.
        choice: ChoiceName,
        /// The values the definition offers.
        allowed: String,
    },
    /// The site file selects a choice the definition does not declare.
    #[error(
        "{path}: choices.{choice}: the machine definition declares no such choice (it declares: {declared}); remove it from [choices]"
    )]
    UnknownChoice {
        /// The site file.
        path: String,
        /// The undeclared choice.
        choice: ChoiceName,
        /// The choices the definition declares.
        declared: String,
    },
    /// The site file selects a value the choice does not offer.
    #[error("{path}: choices.{choice}: {value} is not one of its values: {allowed}")]
    ChoiceValue {
        /// The site file.
        path: String,
        /// The choice.
        choice: ChoiceName,
        /// The refused value.
        value: ChoiceName,
        /// The values the definition offers.
        allowed: String,
    },
    /// Two choices select the same building block; one block has one owner.
    #[error(
        "{path}: choices.{second}: {} is already selected by choice {first}; one building block belongs to one choice",
        block.key()
    )]
    ChoiceOverlap {
        /// The definition.
        path: String,
        /// The choice declared first.
        first: ChoiceName,
        /// The later choice.
        second: ChoiceName,
        /// The block both select.
        block: ChoiceBlock,
    },
    /// The overrides file replaces a block a choice selects.
    #[error(
        "{path}: {}: the machine definition's choice {choice} selects it; keep it in one place: remove it here or choose the value in Setup",
        block.key()
    )]
    ChoiceOverridden {
        /// The overrides file.
        path: String,
        /// The choice that owns the block.
        choice: ChoiceName,
        /// The block replaced twice.
        block: ChoiceBlock,
    },
    /// One combination of the definition's choices gives a stack the
    /// protocol code refuses.
    #[error("{path}: choices {combination}: this combination cannot run: {source}")]
    ChoiceCombination {
        /// The definition, or the site file for the lab's own combination.
        path: String,
        /// The combination as `name=value` pairs.
        combination: String,
        /// Why the stack refuses it.
        #[source]
        source: Box<Error>,
    },
    /// A site file's lab app cannot be chosen, or `lab-apps.toml` is refused.
    #[error("{path}: {problem}")]
    LabApp {
        /// The site file or lab apps file.
        path: String,
        /// What to correct.
        problem: LabAppProblem,
    },
    /// Canonical snapshot could not be serialized.
    #[error("effective config: cannot serialize snapshot: {0}")]
    Serialization(#[from] toml::ser::Error),
}

fn invalid(path: &str, key: &str, reason: &'static str) -> Error {
    Error::Invalid {
        path: path.into(),
        key: key.into(),
        reason,
    }
}

fn load<T: for<'de> Deserialize<'de>>(path: &str, text: &str) -> Result<T, Error> {
    let value = parse_table(path, text)?;
    load_table(path, value, Some(text))
}

fn parse_table(path: &str, text: &str) -> Result<toml::Table, Error> {
    if text.len() > MAX_FILE_BYTES {
        return Err(Error::FileSize { path: path.into() });
    }
    toml::from_str(text).map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })
}

fn load_table<T: for<'de> Deserialize<'de>>(
    path: &str,
    mut value: toml::Table,
    original: Option<&str>,
) -> Result<T, Error> {
    let machine = value.contains_key("protocol");
    let (supported, reason) = if machine {
        (2, "unsupported schema; use machine schema_version = 2")
    } else {
        (1, "use supported schema_version = 1")
    };
    if value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(supported)
    {
        return Err(invalid(path, "schema_version", reason));
    }
    let filled = machine && choices::prepare(&mut value, path)?;
    if machine {
        for section in ["framing", "dialog", "syntax"] {
            if value.get(section).and_then(|v| v.get("kind")).is_some() {
                return Err(invalid(
                    path,
                    &format!("{section}.kind"),
                    "schema 2 derives this selector; remove the key",
                ));
            }
        }
    }
    if value
        .get("deviations")
        .and_then(toml::Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("name").is_some()
                    || item.get("key").is_some()
                    || item.get("reason").is_some()
            })
        })
    {
        return Err(Error::Departure {
            path: path.into(),
            rule: "deviations".into(),
            reason: DepartureRefusal::LegacyDeclaration,
            source: None,
        });
    }
    if let Some(identity) = value.get("mappings").and_then(|v| v.get("identity")) {
        for setting in ["order_id", "order_namespace"] {
            if identity.get(setting).is_none() {
                return Err(Error::MissingIdentitySetting {
                    path: path.into(),
                    setting,
                });
            }
        }
    }
    let parsed = match original.filter(|_| !filled) {
        // Runtime diagnostics keep source positions where no values were filled.
        Some(text) => toml::from_str(text),
        None => toml::Value::Table(value).try_into(),
    };
    parsed.map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })
}

/// The parsed definition facts used to ask for site settings before a machine
/// can run. Runtime validation still checks the completed settings stack.
#[derive(Debug)]
pub struct DefinitionDetails {
    /// The identity and evidence of a complete definition; absent on a capture profile.
    pub identity: Option<(analyte_model::Identifier, EvidenceLevel)>,
    /// The definition's protocol key.
    pub protocol: String,
    /// The transport these settings describe.
    pub kind: TransportKind,
    /// The manual's serial settings, which the lab can override.
    pub serial: Option<SerialValues>,
    /// Choices the normal connection asks the lab to make.
    pub choices: Vec<MachineChoice>,
    /// Whether the definition requires a site polling interval.
    pub asks_autolyser_poll: bool,
    /// Whether the definition requires a site VIDAS patient namespace.
    pub asks_vidas_patient_namespace: bool,
    /// The analyzer mode the capture profile asks the lab to choose, if any.
    pub analyzer_mode: Option<AnalyzerMode>,
}

impl DefinitionDetails {
    /// Reads the typed setup facts from a machine definition or its record-only profile.
    /// # Errors
    /// Refuses oversized or malformed files, unsupported versions and invalid declared choices.
    pub fn parse(path: &str, text: &str) -> Result<Self, Error> {
        let table = parse_table(path, text)?;
        if recording::is_record_file(&table) {
            return recording::definition_details(path, table);
        }
        let document: MachineDocument = load_table(path, table, None)?;
        choices::check(&document, path)?;
        Ok(Self {
            asks_autolyser_poll: uses_site_autolyser_poll(&document),
            asks_vidas_patient_namespace: uses_site_vidas_namespace(&document),
            identity: Some((document.name, document.evidence.level)),
            protocol: document.protocol,
            kind: document.transport.kind,
            serial: document.transport.serial,
            choices: document.choices,
            analyzer_mode: None,
        })
    }
}

/// Parsed layers awaiting semantic validation; cannot be activated.
#[derive(Debug)]
pub struct Draft {
    machine: MachineDocument,
    site: Site,
    source: LabAppSource,
    /// The site file's serial cable choice, applied over the definition's suggestion.
    serial: Option<SerialValues>,
    /// The site file's value for each of the definition's choices.
    choices: ChoiceSelections,
    autolyser_poll_seconds: Option<u32>,
    vidas_patient_namespace: Option<analyte_model::Identifier>,
    overrides: Overrides,
    paths: [String; 3],
}

/// Validated machine metadata and its one supported protocol stack.
/// Only validation can construct this type; it cannot be deserialized or mutated in a snapshot.
#[derive(Debug)]
pub struct Machine {
    /// Definition identity.
    pub name: analyte_model::Identifier,
    /// Named, validated compatibility departures.
    pub deviations: Vec<MachineDeparture>,
    /// Declared evidence; validation never promotes it.
    pub evidence: Evidence,
    pub(crate) software_version: Option<software_version::Check>,
    stack: Stack,
}

impl Machine {
    /// Returns the machine's own test code system (ADR 0029); none for a
    /// record-only connection, which reads no result (ADR 0034).
    #[must_use]
    pub fn test_system(&self) -> Option<&analyte_model::Identifier> {
        self.stack.code_tables().map(|(system, _)| system)
    }

    /// Returns the unit translation table for this stack; empty for a
    /// record-only connection.
    #[must_use]
    pub fn units(&self) -> &[UnitMapping] {
        self.stack.code_tables().map_or(&[], |(_, units)| units)
    }
}

/// Fully checked runtime settings, separate from the unchanged provenance document.
#[derive(Debug)]
pub struct Effective {
    machine: Machine,
    site: Site,
    source: LabAppSource,
    /// The definition's own serial values, before the site's choice.
    suggestion: Option<SerialValues>,
    /// Each declared choice as it resolved.
    choices: Vec<Chosen>,
    document: toml::Value,
}

impl Serialize for Effective {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.document.serialize(serializer)
    }
}

#[derive(Serialize)]
struct Canonical<'a> {
    schema_version: u32,
    machine: &'a MachineDocument,
    site: &'a Site,
    /// The value in use for each declared choice; absent without choices, so
    /// earlier snapshots keep their bytes.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    choices: std::collections::BTreeMap<&'a str, &'a str>,
}

/// Immutable validated snapshot with exact canonical bytes and SHA-256.
#[derive(Debug)]
pub struct Validated {
    effective: Effective,
    canonical: String,
    digest: [u8; 32],
}

/// An activated snapshot; activation performs no delivery or machine operation.
#[derive(Debug)]
pub struct Active(Validated);

impl Draft {
    /// Loads three independently versioned TOML layers, without reading files or
    /// secrets, for a site file that writes its own `[destination]`: the same as
    /// [`Draft::load_with`] with no lab apps listed.
    /// # Errors
    /// Rejects oversized files, unsupported versions, malformed TOML and unknown keys.
    pub fn load(
        machine: (&str, &str),
        site: (&str, &str),
        overrides: (&str, &str),
    ) -> Result<Self, Error> {
        Self::load_with(machine, site, overrides, &LabApps::none(LAB_APPS_FILE))
    }

    /// Loads the three layers and resolves the site file's lab app from its own
    /// `[destination]`, its `lab_app` name in `lab_apps`, or the only lab app.
    /// # Errors
    /// Rejects what [`Draft::load`] rejects, an unknown or missing lab app, and
    /// several lab apps without a choice.
    pub fn load_with(
        machine: (&str, &str),
        site: (&str, &str),
        overrides: (&str, &str),
        lab_apps: &LabApps,
    ) -> Result<Self, Error> {
        recording::refuse_record_file(machine.0, machine.1)?;
        let (resolved, source, chosen) = retention::load_site(site.0, site.1, lab_apps)?;
        Ok(Self {
            machine: load(machine.0, machine.1)?,
            site: resolved,
            source,
            serial: chosen.serial,
            choices: chosen.choices,
            autolyser_poll_seconds: chosen.autolyser_poll_seconds,
            vidas_patient_namespace: chosen.vidas_patient_namespace,
            overrides: load(overrides.0, overrides.1)?,
            paths: [machine.0.into(), site.0.into(), overrides.0.into()],
        })
    }

    /// Applies explicit replacements, checks bounded local evidence, then freezes the exact effective snapshot.
    /// # Errors
    /// Rejects incompatible stacks, invalid endpoints, ambiguous mappings and unbounded settings.
    pub fn validate(mut self) -> Result<Validated, Error> {
        let site_path = self.paths.get(1).map_or("site", String::as_str);
        recording::refuse_recording_site(&self.site, site_path)?;
        let o = self.overrides;
        let path = self.paths.get(2).map_or("overrides", String::as_str);
        let suggestion = self.machine.transport.serial.clone();
        if let Some(chosen) = self.serial {
            choose_serial(&mut self.machine.transport, &chosen, site_path)?;
            if o.transport.is_some() {
                return Err(invalid(
                    path,
                    "transport",
                    "the site file chooses the serial settings ([serial]); keep them in one place: remove [transport] here or [serial] from the site file",
                ));
            }
        }
        let machine_path = self.paths.first().map_or("machine", String::as_str);
        resolve_autolyser_poll(&mut self.machine, self.autolyser_poll_seconds, site_path)?;
        choices::check(&self.machine, machine_path)?;
        let resolved = choices::resolve(&self.machine.choices, &self.choices, site_path)?;
        choices::refuse_overrides(&self.machine.choices, &o, path)?;
        let selected: Vec<usize> = resolved.iter().map(|(index, _)| *index).collect();
        // Validate original layers too: an override must not conceal an invalid
        // shipped definition, nor any combination of its choices.
        choices::validate_offered(
            &self.machine,
            &selected,
            o.transport.is_some(),
            [machine_path, site_path],
        )?;
        let chosen = choices::chosen(&self.machine.choices, &resolved);
        let software_version = software_version::resolve(&self.machine, &chosen, machine_path)?;
        let declares_vidas_namespace = uses_site_vidas_namespace(&self.machine);
        self.machine = choices::applied(&self.machine, &selected);
        validate_site(&self.site, &self.machine.transport, site_path)?;
        apply_overrides(&mut self.machine, &mut self.site, &mut self.source, o, path)?;
        if let Some(orders) = &mut self.machine.orders
            && orders.vidas_patient_namespace_from_site
        {
            if !declares_vidas_namespace || orders.patient_namespace.is_some() {
                return Err(invalid(
                    path,
                    "orders.vidas_patient_namespace_from_site",
                    "declare the site namespace in the machine definition without a fixed namespace",
                ));
            }
            orders.patient_namespace = Some(self.vidas_patient_namespace.ok_or_else(|| {
                invalid(
                    site_path,
                    "vidas_patient_namespace",
                    "enter the patient namespace in Setup",
                )
            })?);
            orders.vidas_patient_namespace_from_site = false;
        } else if self.vidas_patient_namespace.is_some() && !declares_vidas_namespace {
            return Err(invalid(
                site_path,
                "vidas_patient_namespace",
                "the machine definition does not request this setting",
            ));
        }
        let stack = validate_machine(
            &self.machine,
            path,
            self.paths.first().map_or("machine", String::as_str),
        )?;
        validate_site(&self.site, &self.machine.transport, path)?;
        // Preserve the existing sorted TOML layout, including explicit input settings
        // that have no runtime role. Runtime consumers cannot access this document.
        let document = toml::Value::try_from(Canonical {
            schema_version: 1,
            machine: &self.machine,
            site: &self.site,
            choices: chosen
                .iter()
                .map(|chosen| (chosen.name.as_str(), chosen.value.as_str()))
                .collect(),
        })?;
        let canonical = toml::to_string(&document)?;
        let effective = Effective {
            machine: Machine {
                name: self.machine.name,
                deviations: self.machine.deviations,
                evidence: self.machine.evidence,
                software_version,
                stack,
            },
            site: self.site,
            source: self.source,
            suggestion,
            choices: chosen,
            document,
        };
        let digest = Sha256::digest(canonical.as_bytes()).into();
        Ok(Validated {
            effective,
            canonical,
            digest,
        })
    }
}

/// Applies the local override layer's explicit replacements; sections replace in full.
/// # Errors
/// Rejects an override endpoint choice that names both or neither port kinds.
fn apply_overrides(
    machine: &mut MachineDocument,
    site: &mut Site,
    source: &mut LabAppSource,
    o: Overrides,
    path: &str,
) -> Result<(), Error> {
    if o.endpoint.is_some() || o.usb_serial.is_some() {
        site.endpoint = Endpoint::choose(o.endpoint, o.usb_serial, path)?;
    }
    if let Some(v) = o.law {
        machine.law = Some(v);
    }
    if let Some(v) = o.transport {
        machine.transport = v;
    }
    if let Some(v) = o.framing {
        machine.framing = v;
    }
    if let Some(v) = o.dialog {
        machine.dialog = v;
    }
    if let Some(v) = o.syntax {
        machine.syntax = v;
    }
    if let Some(v) = o.mappings {
        machine.mappings = Some(v);
    }
    if let Some(v) = o.orders {
        machine.orders = Some(v);
    }
    if let Some(v) = o.departure_scope {
        machine.departure_scope = Some(v);
    }
    if let Some(v) = o.deviations {
        machine.deviations = v;
    }
    if let Some(v) = o.destination {
        site.destination = Some(v);
        *source = LabAppSource::Overrides;
    }
    Ok(())
}

fn uses_site_vidas_namespace(document: &MachineDocument) -> bool {
    document.vidas.is_some()
        && document
            .orders
            .as_ref()
            .is_some_and(|orders| orders.vidas_patient_namespace_from_site)
}

/// Indicates whether Setup can collect a site namespace for VIDAS work lists.
/// # Errors
/// Refuses malformed machine definitions.
pub fn asks_vidas_patient_namespace(path: &str, text: &str) -> Result<bool, Error> {
    let document = load(path, text)?;
    Ok(uses_site_vidas_namespace(&document))
}

/// Allowed normal Autolyser poll interval in whole seconds.
pub const AUTOLYSER_POLL_SECONDS: std::ops::RangeInclusive<u32> = 1..=300;

/// Reports whether the model requires the lab to supply the normal poll interval.
/// # Errors
/// Refuses malformed machine definitions.
pub fn asks_autolyser_poll(path: &str, text: &str) -> Result<bool, Error> {
    let document: MachineDocument = load(path, text)?;
    Ok(uses_site_autolyser_poll(&document))
}

fn uses_site_autolyser_poll(document: &MachineDocument) -> bool {
    document
        .autolyser
        .as_ref()
        .is_some_and(|settings| matches!(settings.poll_ms, schema::PollInterval::Site(_)))
}

fn resolve_autolyser_poll(
    document: &mut MachineDocument,
    seconds: Option<u32>,
    path: &str,
) -> Result<(), Error> {
    let interval = document
        .autolyser
        .as_mut()
        .map(|settings| &mut settings.poll_ms);
    match (interval, seconds) {
        (Some(interval @ schema::PollInterval::Site(_)), Some(seconds))
            if AUTOLYSER_POLL_SECONDS.contains(&seconds) =>
        {
            *interval = schema::PollInterval::Milliseconds(seconds.saturating_mul(1000));
        }
        (Some(schema::PollInterval::Site(_)), seconds) => {
            return Err(invalid(
                path,
                "autolyser_poll_seconds",
                if seconds.is_none() {
                    "no value; enter the poll interval in Setup step 2 or with --poll-seconds"
                } else {
                    "enter a whole number from 1 to 300"
                },
            ));
        }
        (_, Some(_)) => {
            return Err(invalid(
                path,
                "autolyser_poll_seconds",
                "the machine definition does not request this setting",
            ));
        }
        (_, None) => {}
    }
    Ok(())
}

/// Loads one machine's validated snapshot the way the program does: a site
/// file with `[recording]` is a record-only connection built from the
/// model's capture profile (ADR 0034), without overrides; any other is the
/// definition, site and overrides layers.
/// # Errors
/// Rejects what [`recording::load`] or [`Draft::load_with`] and
/// [`Draft::validate`] reject, and overrides beside a record-only site file.
pub fn load_machine(
    definition: (&str, &str),
    site: (&str, &str),
    overrides: Option<(&str, &str)>,
    lab_apps: &LabApps,
) -> Result<Validated, Error> {
    if recording::records_only(site.1) {
        if let Some((path, _)) = overrides {
            return Err(invalid(
                path,
                "overrides",
                "a record-only machine uses its capture profile as shipped; remove the overrides file from its entry",
            ));
        }
        return recording::load(definition, site);
    }
    Draft::load_with(
        definition,
        site,
        overrides.unwrap_or(("overrides", "schema_version = 1")),
        lab_apps,
    )?
    .validate()
}

impl SerialValues {
    /// Refuses a value outside the standard lists, as loading a site file's
    /// `[serial]` does; for a writer that checks before the machine's model is
    /// known. Absent values are not checked.
    /// # Errors
    /// Names the `serial.<setting>` key and the values allowed.
    pub fn check(&self, path: &str) -> Result<(), Error> {
        validation::check_serial(self, path, "serial")
    }
}

/// Applies the site's serial `chosen` values over the definition's, after
/// checking each against the standard list; only a serial machine takes them.
pub(crate) fn choose_serial(
    transport: &mut Transport,
    chosen: &SerialValues,
    site_path: &str,
) -> Result<(), Error> {
    if transport.kind != TransportKind::Serial {
        return Err(invalid(
            site_path,
            "serial",
            "only a machine on a serial cable takes serial settings; remove [serial]",
        ));
    }
    chosen.check(site_path)?;
    transport.serial = Some(chosen.over(transport.serial.as_ref()));
    Ok(())
}

impl Validated {
    /// Returns immutable, checked protocol and site selections.
    #[must_use]
    pub fn effective(&self) -> &Effective {
        &self.effective
    }

    /// Returns the exact bytes to retain beside every receipt.
    #[must_use]
    pub fn canonical(&self) -> &str {
        &self.canonical
    }

    /// Returns SHA-256 of exactly `canonical().as_bytes()`.
    #[must_use]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Consumes a validated snapshot. The host must call only between conversations (ADR 0002).
    #[must_use]
    pub fn activate(self) -> Active {
        Active(self)
    }
}

impl Active {
    /// Returns the immutable active snapshot; queued deliveries remain owned by the host.
    #[must_use]
    pub fn snapshot(&self) -> &Validated {
        &self.0
    }
}

impl Effective {
    /// Returns the single supported stack built from the effective validated layers.
    #[must_use]
    pub fn stack(&self) -> &Stack {
        &self.machine.stack
    }

    /// Returns validated identity, deviations and evidence; protocol settings are in `stack()`.
    #[must_use]
    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// Returns validated installation settings, with credential references only.
    #[must_use]
    pub fn site(&self) -> &Site {
        &self.site
    }

    /// The serial values the machine definition itself writes (the manual's
    /// suggestion), before the site file's choice; none for other links or a
    /// definition that writes none. The stack holds the values in use.
    #[must_use]
    pub fn serial_suggestion(&self) -> Option<&SerialValues> {
        self.suggestion.as_ref()
    }

    /// Each choice the machine definition declares, with the value in use and
    /// whether the site file chose it or the suggestion applies (ADR 0032);
    /// empty when it declares none.
    #[must_use]
    pub fn choices(&self) -> &[Chosen] {
        &self.choices
    }

    /// Whether this is a record-only connection: it keeps the analyzer's
    /// conversation and sends nothing to a lab app (ADR 0034).
    #[must_use]
    pub const fn records_only(&self) -> bool {
        matches!(self.machine.stack, Stack::Record(_))
    }

    /// Where the lab app in `site().destination` was written. It is not part of
    /// the snapshot: the same settings give the same bytes from any source.
    #[must_use]
    pub fn lab_app_source(&self) -> LabAppSource {
        self.source
    }
}
