//! Loads bounded configuration and validates settings and local evidence without connection I/O.
#![forbid(unsafe_code)]

mod https;
pub use https::{HttpsUrl, loopback_test_url};
mod orders;
mod retention;
pub use retention::Retention;
mod selection;
use selection::ParsedMachine;
pub use selection::{StackChoice, SyntaxKind};
mod schema;
mod stack;
mod validation;
pub use orders::*;
pub use schema::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use stack::*;
use validation::{validate_machine, validate_site};

/// Maximum bytes accepted per TOML layer, before parsing or allocation.
pub const MAX_FILE_BYTES: usize = 1_048_576;

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
        "{path}: syntax.edition: HL7 {edition} is recognized but not yet supported for result processing; supported: 2.3, 2.3.1, 2.4, 2.5, 2.5.1, 2.6, 2.8.2, 2.9.1"
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
    if text.len() > MAX_FILE_BYTES {
        return Err(Error::FileSize { path: path.into() });
    }
    let value: toml::Table = toml::from_str(text).map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })?;
    if value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        return Err(invalid(
            path,
            "schema_version",
            "use supported schema_version = 1",
        ));
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
    toml::from_str(text).map_err(|source| Error::Schema {
        path: path.into(),
        source,
    })
}

/// Parsed layers awaiting semantic validation; cannot be activated.
#[derive(Debug)]
pub struct Draft {
    machine: MachineDocument,
    site: Site,
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
    stack: Stack,
}

impl Machine {
    /// Returns the required test translation table for this stack.
    #[must_use]
    pub fn tests(&self) -> &[TestMapping] {
        self.stack.code_tables().0
    }

    /// Returns the required unit translation table for this stack.
    #[must_use]
    pub fn units(&self) -> &[UnitMapping] {
        self.stack.code_tables().1
    }
}

/// Fully checked runtime settings, separate from the unchanged provenance document.
#[derive(Debug)]
pub struct Effective {
    machine: Machine,
    site: Site,
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
    /// Loads three independently versioned TOML layers, without reading files or secrets.
    /// # Errors
    /// Rejects oversized files, unsupported versions, malformed TOML and unknown keys.
    pub fn load(
        machine: (&str, &str),
        site: (&str, &str),
        overrides: (&str, &str),
    ) -> Result<Self, Error> {
        Ok(Self {
            machine: load(machine.0, machine.1)?,
            site: retention::load_site(site.0, site.1)?,
            overrides: load(overrides.0, overrides.1)?,
            paths: [machine.0.into(), site.0.into(), overrides.0.into()],
        })
    }

    /// Applies explicit replacements, checks bounded local evidence, then freezes the exact effective snapshot.
    /// # Errors
    /// Rejects incompatible stacks, invalid endpoints, ambiguous mappings and unbounded settings.
    pub fn validate(mut self) -> Result<Validated, Error> {
        // Validate original layers too: an override must not conceal an invalid shipped definition.
        validate_machine(
            &self.machine,
            self.paths.first().map_or("machine", String::as_str),
            self.paths.first().map_or("machine", String::as_str),
        )?;
        validate_site(
            &self.site,
            &self.machine.transport,
            self.paths.get(1).map_or("site", String::as_str),
        )?;
        let o = self.overrides;
        if let Some(v) = o.law {
            self.machine.law = Some(v);
        }
        if let Some(v) = o.transport {
            self.machine.transport = v;
        }
        if let Some(v) = o.framing {
            self.machine.framing = v;
        }
        if let Some(v) = o.dialog {
            self.machine.dialog = v;
        }
        if let Some(v) = o.syntax {
            self.machine.syntax = v;
        }
        if let Some(v) = o.mappings {
            self.machine.mappings = Some(v);
        }
        if let Some(v) = o.orders {
            self.machine.orders = Some(v);
        }
        if let Some(v) = o.departure_scope {
            self.machine.departure_scope = Some(v);
        }
        if let Some(v) = o.deviations {
            self.machine.deviations = v;
        }
        if let Some(v) = o.endpoint {
            self.site.endpoint = v;
        }
        if let Some(v) = o.destination {
            self.site.destination = v;
        }
        let path = self.paths.get(2).map_or("overrides", String::as_str);
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
        })?;
        let canonical = toml::to_string(&document)?;
        let effective = Effective {
            machine: Machine {
                name: self.machine.name,
                deviations: self.machine.deviations,
                evidence: self.machine.evidence,
                stack,
            },
            site: self.site,
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
}

impl Effective {
    /// Returns LIS02 parser limits only for ASTM record stacks.
    #[must_use]
    pub fn astm_limits(&self) -> Option<analyte_astm::Limits> {
        match &self.machine.stack {
            Stack::Astm(RecordTransport::Stream(s)) => Some(s.syntax().parser_limits()),
            Stack::Astm(RecordTransport::Folder(s)) => Some(s.syntax().parser_limits()),
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::VidasLiteral(_)
            | Stack::Autolyser(_)
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }

    /// Returns negotiated HL7 options, including LAW's fixed policy, only for HL7 stacks.
    #[must_use]
    pub fn hl7_options(&self) -> Option<analyte_hl7::Options> {
        match &self.machine.stack {
            Stack::Hl7(RecordTransport::Stream(s)) => Some(s.syntax().options()),
            Stack::Hl7(RecordTransport::Folder(s)) => Some(s.syntax().options()),
            Stack::Law(s) => Some(s.options()),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Astm(RecordTransport::Folder(_))
            | Stack::Poct(_)
            | Stack::VidasLiteral(_)
            | Stack::Autolyser(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
