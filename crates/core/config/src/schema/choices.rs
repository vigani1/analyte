//! Holds the choices a machine definition declares and the lab's selections of them (ADR 0032).
//!
//! A choice is a setting the analyzer itself offers (a mode on its own screen)
//! or the installed software version. Each value names a closed set of
//! existing building blocks; nothing here is a field path or an expression.
use super::{AckPoint, Encoding, Mappings, VidasSampleLimit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most choices one definition declares.
pub const MAX_CHOICES: usize = 16;
/// Most values one choice offers.
pub const MAX_CHOICE_VALUES: usize = 16;
/// Most combinations of values a definition may offer; each is checked at load.
pub const MAX_CHOICE_COMBINATIONS: usize = 64;

/// A choice or value name: 1 to 64 lower-case ASCII letters, digits, `-` or
/// `_`, starting with a letter or digit, so it is the same key in the site
/// file, on the command line (`--choice name=value`) and in the Setup form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChoiceName(String);

impl ChoiceName {
    /// Longest name in bytes.
    pub const MAX_BYTES: usize = 64;

    /// Checks `text` without trimming or changing case.
    /// # Errors
    /// Says which rule `text` breaks.
    pub fn new(text: &str) -> Result<Self, &'static str> {
        let first_ok = text
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if !first_ok
            || text.len() > Self::MAX_BYTES
            || !text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        {
            return Err(
                "a choice or value name is 1 to 64 lower-case letters, digits, - or _, starting with a letter or digit",
            );
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ChoiceName {
    type Error = &'static str;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(&text)
    }
}

impl From<ChoiceName> for String {
    fn from(name: ChoiceName) -> Self {
        name.0
    }
}

impl std::fmt::Display for ChoiceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Plain words for people: 1 to 200 bytes, one line, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChoiceLabel(String);

impl ChoiceLabel {
    /// Longest label in bytes.
    pub const MAX_BYTES: usize = 200;

    /// The exact words.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ChoiceLabel {
    type Error = &'static str;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.trim().is_empty()
            || text.len() > Self::MAX_BYTES
            || text.chars().any(char::is_control)
        {
            return Err("a label is 1 to 200 bytes of plain words on one line");
        }
        Ok(Self(text))
    }
}

impl From<ChoiceLabel> for String {
    fn from(label: ChoiceLabel) -> Self {
        label.0
    }
}

/// What kind of question a choice is, which only changes how Setup asks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    /// A mode the analyzer offers on its own settings screen.
    Mode,
    /// The installed software version, read from the analyzer's About screen;
    /// its values are version bands whose facts follow from it. At most one
    /// per definition.
    SoftwareVersion,
}

/// Whether the definition's `[orders]` apply: a "send results only" mode
/// turns them off; it never writes order settings of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdersSwitch {
    /// The definition's `[orders]` apply as written.
    On,
    /// No queries and no work lists, as if `[orders]` were absent.
    Off,
}

/// The VIDAS settings a choice may select; every other `[vidas]` key stays
/// the definition's fact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VidasChoice {
    /// Literal (`false`) or Alternate (`true`) acknowledgment point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternate: Option<AckPoint>,
    /// The sample ID ceiling of the installed VIDAS PC software.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_limit: Option<VidasSampleLimit>,
}

/// Autolyser settings that can differ between documented operating modes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutolyserChoice {
    /// Selects the installed report layout.
    pub controls: Option<bool>,
    /// Allows recovery with L before the next R.
    pub recover_last_on_poll: Option<bool>,
    /// Allows explicit work-list download.
    pub download: Option<bool>,
}

/// One building block a choice value can select; the closed list of things a
/// lab's choice can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChoiceBlock {
    /// The whole `[mappings]` table: a host dialect's field positions.
    Mappings,
    /// Whether `[orders]` apply.
    Orders,
    /// `vidas.alternate`.
    VidasAlternate,
    /// `vidas.sample_limit`.
    VidasSampleLimit,
    /// The mode's declared departures.
    Departures,
    /// The Autolyser report layout.
    AutolyserControls,
    /// The Autolyser recovery policy.
    AutolyserRecovery,
    /// The Autolyser work-list policy.
    AutolyserDownload,
    /// The protocol's character encoding.
    Encoding,
}

impl ChoiceBlock {
    /// The key a value writes it under, as errors and pages name it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Mappings => "mappings",
            Self::Orders => "orders",
            Self::VidasAlternate => "vidas.alternate",
            Self::VidasSampleLimit => "vidas.sample_limit",
            Self::Departures => "departures",
            Self::AutolyserControls => "autolyser.controls",
            Self::AutolyserRecovery => "autolyser.recover_last_on_poll",
            Self::AutolyserDownload => "autolyser.download",
            Self::Encoding => "syntax.encoding",
        }
    }
}

/// One value of a choice and the building blocks it selects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceValue {
    /// The value's name, as the site file and `--choice` write it.
    pub value: ChoiceName,
    /// The value as the analyzer's screen or manual names it.
    pub label: ChoiceLabel,
    /// Replaces the definition's whole `[mappings]` table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Mappings>,
    /// Keeps or turns off the definition's `[orders]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orders: Option<OrdersSwitch>,
    /// Replaces named `[vidas]` settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vidas: Option<VidasChoice>,
    /// Selects named declarations in the definition's `choice_departures` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departures: Option<Vec<ChoiceName>>,
    /// Selects specific Autolyser settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autolyser: Option<AutolyserChoice>,
    /// Selects the protocol's existing encoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<Encoding>,
}

impl ChoiceValue {
    /// The building blocks this value selects, in [`ChoiceBlock`] order.
    #[must_use]
    pub fn blocks(&self) -> Vec<ChoiceBlock> {
        let vidas = self.vidas.clone().unwrap_or_default();
        let autolyser = self.autolyser.clone().unwrap_or_default();
        [
            (self.mappings.is_some(), ChoiceBlock::Mappings),
            (self.orders.is_some(), ChoiceBlock::Orders),
            (vidas.alternate.is_some(), ChoiceBlock::VidasAlternate),
            (vidas.sample_limit.is_some(), ChoiceBlock::VidasSampleLimit),
            (self.departures.is_some(), ChoiceBlock::Departures),
            (autolyser.controls.is_some(), ChoiceBlock::AutolyserControls),
            (
                autolyser.recover_last_on_poll.is_some(),
                ChoiceBlock::AutolyserRecovery,
            ),
            (autolyser.download.is_some(), ChoiceBlock::AutolyserDownload),
            (self.encoding.is_some(), ChoiceBlock::Encoding),
        ]
        .into_iter()
        .filter_map(|(set, block)| set.then_some(block))
        .collect()
    }
}

/// A choice a machine definition declares in `[[choices]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineChoice {
    /// The choice's name, as the site file's `[choices]` and `--choice` write it.
    pub name: ChoiceName,
    /// A mode on the analyzer's screen or its software version.
    pub kind: ChoiceKind,
    /// The question in plain words, naming where the analyzer shows it.
    pub label: ChoiceLabel,
    /// The value the manual suggests; absent when only the lab can know (a
    /// software version), so the lab must choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested: Option<ChoiceName>,
    /// The allowed values, 2 to 16.
    pub values: Vec<ChoiceValue>,
}

impl MachineChoice {
    /// The value named `name`, if this choice offers it.
    #[must_use]
    pub fn value(&self, name: &ChoiceName) -> Option<&ChoiceValue> {
        self.values.iter().find(|value| &value.value == name)
    }

    /// The allowed value names, joined for an error or a hint.
    #[must_use]
    pub fn allowed(&self) -> String {
        self.values
            .iter()
            .map(|value| value.value.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The site file's `[choices]`: the lab's value for each choice, by name.
pub type ChoiceSelections = BTreeMap<ChoiceName, ChoiceName>;
