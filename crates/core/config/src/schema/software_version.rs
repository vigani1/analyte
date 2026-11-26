//! Declares and resolves evidenced analyzer software version checks.
use crate::schema::{ChoiceKind, ChoiceName, MachineDocument, VersionEvidence};
use crate::{Chosen, Error, Machine, invalid};
use analyte_model::Identifier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Supported wire locations. Each distinction must be backed by vendor evidence.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    /// ASTM H.5, third sender component (Biolyte service manual 4.2A, Appendix VI).
    AstmSenderThirdComponent,
    /// HL7 MSH-3, first component when a vendor places software there.
    Hl7SendingApplication,
    /// HL7 MSH-4, first component when a vendor places software there.
    Hl7SendingFacility,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Declaration {
    selector: Selector,
    source: Identifier,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bands: BTreeMap<ChoiceName, Vec<Identifier>>,
}

/// Software version coverage for the settings in use.
#[derive(Debug, Clone)]
pub enum Check {
    /// The selected version has no manual coverage or expected wire tokens.
    Unchecked,
    /// Compare incoming versions to exact tokens backed by evidence.
    Expected(ExpectedVersions),
}

impl Check {
    /// The wire comparison, when the active version has expected tokens.
    #[must_use]
    pub const fn expected(&self) -> Option<&ExpectedVersions> {
        match self {
            Self::Unchecked => None,
            Self::Expected(expected) => Some(expected),
        }
    }
}

/// The validated selector and the expected tokens for a wire comparison.
#[derive(Debug, Clone)]
pub struct ExpectedVersions {
    /// The selected location in incoming messages.
    pub selector: Selector,
    /// Exact checked versions; no ordering or prefix matching is inferred.
    pub checked: Vec<Identifier>,
    /// Identity of the definition and checked versions, independent of the site link.
    pub revision: analyte_model::Digest,
}

impl Machine {
    /// Returns the coverage and wire check, if applicable to the active settings.
    #[must_use]
    pub fn software_version(&self) -> Option<&Check> {
        self.software_version.as_ref()
    }
}

/// Accepts only a bounded wire token containing an ASCII digit. Unexpected text
/// stays unknown and is never copied into operator messages or notifications.
#[must_use]
pub fn reported(raw: &[u8]) -> Option<Identifier> {
    if raw.len() > 64
        || !raw.iter().any(u8::is_ascii_digit)
        || !raw
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(b))
    {
        return None;
    }
    Identifier::new(std::str::from_utf8(raw).ok()?).ok()
}

fn validate(document: &MachineDocument, path: &str) -> Result<(), Error> {
    let Some(declaration) = &document.software_version else {
        return Ok(());
    };
    let refuse = |why| invalid(path, "software_version", why);
    let compatible = match declaration.selector {
        Selector::AstmSenderThirdComponent => document.protocol == "astm",
        Selector::Hl7SendingApplication | Selector::Hl7SendingFacility => {
            matches!(document.protocol.as_str(), "hl7" | "law")
        }
    };
    if !compatible || declaration.source.as_str().trim().is_empty() {
        return Err(refuse(
            "declare a selector for this protocol and its source",
        ));
    }
    let choice = document
        .choices
        .iter()
        .find(|c| c.kind == ChoiceKind::SoftwareVersion);
    if let Some(choice) = choice {
        let mut expected = choice
            .values
            .iter()
            .filter(|value| value.version_evidence == VersionEvidence::Checked);
        if declaration.bands.len() != expected.clone().count()
            || expected.any(|value| !declaration.bands.contains_key(&value.value))
        {
            return Err(refuse(
                "declare wire tokens for every checked software version value and omit the unchecked value",
            ));
        }
        for versions in declaration.bands.values() {
            if versions.is_empty()
                || versions.len() > 128
                || versions.iter().any(|version| {
                    reported(version.as_str().as_bytes()).is_none()
                        || !document
                            .evidence
                            .capabilities
                            .iter()
                            .any(|c| c.firmware == *version)
                })
            {
                return Err(refuse(
                    "each band must name 1 to 128 checked versions from evidence",
                ));
            }
        }
    } else if !declaration.bands.is_empty()
        || !document
            .evidence
            .capabilities
            .iter()
            .any(|c| reported(c.firmware.as_str().as_bytes()).is_some())
    {
        return Err(refuse(
            "declare checked firmware evidence, with bands only for a software version choice",
        ));
    }
    Ok(())
}

pub(crate) fn resolve(
    document: &MachineDocument,
    chosen: &[Chosen],
    path: &str,
) -> Result<Option<Check>, Error> {
    validate(document, path)?;
    let choice = chosen
        .iter()
        .find(|c| c.kind == ChoiceKind::SoftwareVersion);
    if choice.is_some_and(|c| c.version_evidence == VersionEvidence::Unchecked) {
        return Ok(Some(Check::Unchecked));
    }
    let Some(declaration) = &document.software_version else {
        return Ok(None);
    };
    let mut checked = match choice {
        Some(choice) => declaration
            .bands
            .get(&choice.value)
            .cloned()
            .ok_or_else(|| {
                invalid(
                    path,
                    "software_version.bands",
                    "the selected software version band has no checked versions",
                )
            })?,
        None => document
            .evidence
            .capabilities
            .iter()
            .filter_map(|c| reported(c.firmware.as_str().as_bytes()))
            .collect(),
    };
    checked.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    checked.dedup();
    let mut digest = Sha256::new();
    digest.update(toml::to_string(document)?);
    if let Some(choice) = choice {
        digest.update(choice.value.as_str());
    }
    Ok(Some(Check::Expected(ExpectedVersions {
        selector: declaration.selector,
        checked,
        revision: analyte_model::Digest(digest.finalize().into()),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADRs 0032 and 0036: a selected band supplies the exact evidenced versions.
    #[test]
    fn selected_software_band_excludes_versions_from_other_bands()
    -> Result<(), Box<dyn std::error::Error>> {
        let astm = include_str!("../../examples/astm.toml")
            .replace("firmware = \"unknown\"", "firmware = \"1.0\"");
        let (_, capability) = astm
            .split_once("[[evidence.capabilities]]")
            .ok_or("evidence fixture")?;
        let other = capability.replace("firmware = \"1.0\"", "firmware = \"2.0\"");
        let orders = include_str!("../../examples/orders.toml");
        let definition = format!(
            "{astm}\n[[evidence.capabilities]]{other}\n{orders}\n{}",
            r#"
[[choices]]
name = "software_version"
kind = "software_version"
label = "Software version"
[[choices.values]]
value = "one"
label = "Version 1"
orders = "on"
[[choices.values]]
value = "two"
label = "Version 2"
orders = "off"

[software_version]
selector = "astm_sender_third_component"
source = "Synthetic band fixture"
[software_version.bands]
one = ["1.0"]
two = ["2.0"]
"#
        );
        let load = |definition: &str, band: &str| {
            let site = format!(
                "{}\n[choices]\nsoftware_version = '{band}'\n",
                include_str!("../../examples/site.toml")
            );
            crate::Draft::load(
                ("machine.toml", definition),
                ("site.toml", &site),
                ("overrides.toml", "schema_version = 1"),
            )?
            .validate()
        };
        let one = load(&definition, "one")?;
        let two = load(&definition, "two")?;
        let check = |snapshot: &crate::Validated| {
            snapshot
                .effective()
                .machine()
                .software_version()
                .and_then(Check::expected)
                .cloned()
                .ok_or("missing check")
        };
        assert_eq!(check(&one)?.checked, vec![Identifier::new("1.0")?]);
        assert_eq!(check(&two)?.checked, vec![Identifier::new("2.0")?]);
        assert_ne!(check(&one)?.revision, check(&two)?.revision);
        assert!(
            load(
                &definition.replace("two = [\"2.0\"]", "two = [\"9.0\"]"),
                "one"
            )
            .is_err()
        );
        Ok(())
    }

    // ADR 0036 (2026-11-26): the unchecked value is distinct from a checked
    // band, even when the definition has a selector and evidenced tokens.
    #[test]
    fn unchecked_software_band_has_no_wire_expectations() -> Result<(), Box<dyn std::error::Error>>
    {
        let astm = include_str!("../../examples/astm.toml")
            .replace("firmware = \"unknown\"", "firmware = \"1.0\"");
        let choice = include_str!("../../../../testing/fixtures/software-version-choices.toml");
        let definition = format!(
            "{astm}\n{choice}\n{}",
            r#"
[software_version]
selector = "astm_sender_third_component"
source = "Synthetic band fixture"
[software_version.bands]
v1 = ["1.0"]
"#
        );
        let site = format!(
            "{}\n[choices]\nsoftware_version = 'other'\n",
            include_str!("../../examples/site.toml")
        );
        let load = |definition: &str| {
            crate::Draft::load(
                ("machine.toml", definition),
                ("site.toml", &site),
                ("overrides.toml", "schema_version = 1"),
            )?
            .validate()
        };
        let snapshot = load(&definition)?;
        assert!(matches!(
            snapshot.effective().machine().software_version(),
            Some(Check::Unchecked)
        ));
        let invented = load(&format!("{definition}\nother = [\"1.0\"]\n"))
            .expect_err("an unchecked band cannot declare tokens");
        assert!(
            invented.to_string().contains("omit the unchecked value"),
            "{invented}"
        );
        Ok(())
    }
}
