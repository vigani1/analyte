//! Bounded configuration loading and immutable activation snapshots (ADR 0002).
#![forbid(unsafe_code)]

mod schema;
mod validation;
pub use schema::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use validation::{validate_machine, validate_site};

/// Maximum bytes accepted per TOML layer, before parsing or allocation.
pub const MAX_FILE_BYTES: usize = 1_048_576;

/// A config failure names its source file and actionable key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Required identity configuration is absent; no mapping default is invented.
    #[error("{path}: mappings.identity.{setting}: required setting is missing")]
    MissingIdentitySetting {
        /// Source TOML layer.
        path: String,
        /// Missing identity setting.
        setting: &'static str,
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
    machine: Machine,
    site: Site,
    overrides: Overrides,
    paths: [String; 3],
}

/// Fully checked effective settings; fields cannot be mutated or deserialized.
#[derive(Debug, Serialize)]
pub struct Effective {
    schema_version: u32,
    machine: Machine,
    site: Site,
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
            site: load(site.0, site.1)?,
            overrides: load(overrides.0, overrides.1)?,
            paths: [machine.0.into(), site.0.into(), overrides.0.into()],
        })
    }

    /// Applies explicit replacements, validates once, then freezes the exact effective snapshot.
    /// # Errors
    /// Rejects incompatible stacks, invalid endpoints, ambiguous mappings and unbounded settings.
    pub fn validate(mut self) -> Result<Validated, Error> {
        // Validate original layers too: an override must not conceal an invalid shipped definition.
        validate_machine(
            &self.machine,
            self.paths.first().map_or("machine", String::as_str),
        )?;
        validate_site(
            &self.site,
            &self.machine.transport,
            self.paths.get(1).map_or("site", String::as_str),
        )?;
        let o = self.overrides;
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
            self.machine.mappings = v;
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
        validate_machine(&self.machine, path)?;
        validate_site(&self.site, &self.machine.transport, path)?;
        let effective = Effective {
            schema_version: 1,
            machine: self.machine,
            site: self.site,
        };
        // A TOML value table sorts keys; whitespace, comments and input table order cannot affect identity.
        let table = toml::Value::try_from(&effective)?;
        let canonical = toml::to_string(&table)?;
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
    /// Returns validated protocol settings and mappings.
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
    /// Returns LIS02 parser limits only when ASTM syntax was selected.
    #[must_use]
    pub fn astm_limits(&self) -> Option<analyte_astm::Limits> {
        let s = &self.machine.syntax;
        (s.kind == SyntaxKind::AstmLis02).then_some(analyte_astm::Limits {
            bytes: s.bytes,
            record: s.record_bytes,
            fields: s.fields,
            repeats: s.repetitions,
            components: s.components,
        })
    }

    /// Returns the exact HL7 syntax options only when HL7 was selected.
    #[must_use]
    pub fn hl7_options(&self) -> Option<analyte_hl7::Options> {
        let s = &self.machine.syntax;
        if s.kind != SyntaxKind::Hl7V2 {
            return None;
        }
        Some(analyte_hl7::Options {
            charset: match s.encoding {
                Encoding::Ascii => analyte_hl7::Charset::Ascii,
                Encoding::Latin1 => analyte_hl7::Charset::Latin1,
                Encoding::Utf8 => analyte_hl7::Charset::Utf8,
            },
            terminator: match s.terminator? {
                Terminator::Cr => analyte_hl7::Terminator::Cr,
                Terminator::CrLf => analyte_hl7::Terminator::CrLf,
                Terminator::Lf => analyte_hl7::Terminator::Lf,
            },
            escapes: match s.escapes? {
                Escapes::Reject => analyte_hl7::EscapePolicy::Reject,
                Escapes::Preserve => analyte_hl7::EscapePolicy::Preserve,
            },
            limits: analyte_hl7::Limits {
                message_bytes: s.bytes,
                segments: s.records,
                fields: s.fields,
                repetitions: s.repetitions,
            },
        })
    }
}
