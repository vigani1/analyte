//! Checks a definition's declared choices, resolves the lab's selections and applies them to the definition (ADR 0032).
//!
//! The definition declares each choice and its values; the site file's
//! `[choices]` holds the lab's value for each. A choice without a selection
//! takes the definition's suggestion; without either it is refused by name.
//! Every combination of values a definition offers is validated as a whole
//! stack when it loads, so Setup never offers one the protocol code refuses.
use crate::schema::{
    ChoiceBlock, ChoiceKind, ChoiceLabel, ChoiceName, ChoiceSelections, ChoiceValue,
    MAX_CHOICE_COMBINATIONS, MAX_CHOICE_VALUES, MAX_CHOICES, MachineChoice, MachineDocument,
    OrdersSwitch, VersionEvidence,
};
use crate::validation::validate_machine;
use crate::{Error, invalid};
use std::collections::BTreeMap;

/// One declared choice as it resolved: the value in use and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// The choice's name.
    pub name: ChoiceName,
    /// A mode or the software version.
    pub kind: ChoiceKind,
    /// The question in plain words.
    pub label: ChoiceLabel,
    /// The value in use.
    pub value: ChoiceName,
    /// The value in use in the analyzer's words.
    pub value_label: ChoiceLabel,
    /// Whether the selected software version is covered by the manuals.
    pub version_evidence: VersionEvidence,
    /// The definition's suggestion and its words; none when it suggests none.
    pub suggested: Option<(ChoiceName, ChoiceLabel)>,
    /// The site file chose it; otherwise the suggestion applies.
    pub selected: bool,
}

impl Chosen {
    /// The value in use is not the definition's suggestion (or it suggests none).
    #[must_use]
    pub fn differs(&self) -> bool {
        self.suggested
            .as_ref()
            .is_none_or(|(suggested, _)| *suggested != self.value)
    }
}

/// The choices the machine definition `text` declares, read with the
/// loader's own checks, for a page or command that asks them before the
/// machine can load.
/// # Errors
/// Refuses what loading the definition refuses at the schema level, and
/// declarations that break the choice rules.
pub fn declared(path: &str, text: &str) -> Result<Vec<MachineChoice>, Error> {
    let document: MachineDocument = crate::load(path, text)?;
    check(&document, path)?;
    Ok(document.choices)
}

/// TOML sections and keys which can be supplied by a choice.
const BLOCK_KEYS: [(&str, &str); 6] = [
    ("vidas", "alternate"),
    ("vidas", "sample_limit"),
    ("autolyser", "controls"),
    ("autolyser", "recover_last_on_poll"),
    ("autolyser", "download"),
    ("syntax", "encoding"),
];

/// Checks the raw definition before the schema reads it, and returns whether
/// it changed it. Evidence and scope are declared outside the choices.
/// Required keys picked by a choice are read from its first value and
/// replaced with the selected values before validating any stack.
pub(crate) fn prepare(raw: &mut toml::Table, path: &str) -> Result<bool, Error> {
    let Some(choices) = raw.get("choices").and_then(toml::Value::as_array) else {
        return Ok(false);
    };
    let mut owned: Vec<(&str, &str, toml::Value)> = Vec::new();
    let mut owns_mappings = false;
    for choice in choices {
        let values = choice
            .get("values")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for table in std::iter::once(choice).chain(values) {
            for key in ["evidence", "deviations", "departure_scope"] {
                if table.get(key).is_some() {
                    return Err(invalid(
                        path,
                        &format!("choices.{key}"),
                        "a choice cannot change evidence or departures; departures stay in deviations with their own scope and evidence (ADR 0015)",
                    ));
                }
            }
            if let Some(identity) = table.get("mappings").and_then(|m| m.get("identity")) {
                for setting in ["order_id", "order_namespace"] {
                    if identity.get(setting).is_none() {
                        return Err(Error::MissingIdentitySetting {
                            path: path.into(),
                            setting,
                        });
                    }
                }
            }
        }
        let Some(first) = values.first() else {
            continue;
        };
        owns_mappings |= first.get("mappings").is_some();
        for (section, key) in BLOCK_KEYS {
            let value = if section == "syntax" {
                first.get(key)
            } else {
                first.get(section).and_then(|table| table.get(key))
            };
            if let Some(value) = value {
                owned.push((section, key, value.clone()));
            }
        }
    }
    if owns_mappings && raw.contains_key("mappings") {
        return Err(invalid(
            path,
            "mappings",
            "a choice selects the mappings; write them only in the choice's values, so one place decides them",
        ));
    }
    for (section, key, _) in &owned {
        if raw
            .get(*section)
            .and_then(|table| table.get(*key))
            .is_some()
        {
            return Err(invalid(
                path,
                &format!("{section}.{key}"),
                "a choice selects this setting; write it only in the choice's values, so one place decides it",
            ));
        }
    }
    let mut changed = false;
    for (section, key, value) in owned {
        if let Some(table) = raw.get_mut(section).and_then(toml::Value::as_table_mut) {
            table.entry(key).or_insert(value);
            changed = true;
        }
    }
    Ok(changed)
}

/// Checks the declarations against the choice rules and the definition they
/// apply to; nothing about the lab's selection.
pub(crate) fn check(document: &MachineDocument, path: &str) -> Result<(), Error> {
    let choices = &document.choices;
    if choices.len() > MAX_CHOICES {
        return Err(invalid(path, "choices", "declare at most 16 choices"));
    }
    if choices
        .iter()
        .filter(|choice| choice.kind == ChoiceKind::SoftwareVersion)
        .count()
        > 1
    {
        return Err(invalid(
            path,
            "choices",
            "declare at most one software_version choice",
        ));
    }
    let mut owners: BTreeMap<ChoiceBlock, &ChoiceName> = BTreeMap::new();
    let mut combinations: usize = 1;
    for (at, choice) in choices.iter().enumerate() {
        if choices
            .iter()
            .take(at)
            .any(|earlier| earlier.name == choice.name)
        {
            return Err(invalid(
                path,
                &format!("choices.{}", choice.name),
                "each choice name is declared once",
            ));
        }
        combinations = combinations
            .checked_mul(choice.values.len())
            .filter(|count| *count <= MAX_CHOICE_COMBINATIONS)
            .ok_or_else(|| {
                invalid(
                    path,
                    "choices",
                    "the choices offer more than 64 combinations; each is checked when the definition loads",
                )
            })?;
        for block in check_choice(document, choice, path)? {
            if let Some(owner) = owners.insert(block, &choice.name) {
                return Err(Error::ChoiceOverlap {
                    path: path.into(),
                    first: owner.clone(),
                    second: choice.name.clone(),
                    block,
                });
            }
        }
    }
    for declaration in &document.choice_departures {
        let count = document
            .choice_departures
            .iter()
            .filter(|d| {
                d.tag() == declaration.tag()
                    && d.declaration().scope.mode == declaration.declaration().scope.mode
            })
            .count();
        if count != 1
            || !choices
                .iter()
                .flat_map(|choice| &choice.values)
                .any(|value| {
                    value.value.as_str() == declaration.declaration().scope.mode.as_str()
                        && references(value, declaration.tag())
                })
        {
            return Err(invalid(
                path,
                "choice_departures",
                "each scoped declaration must be unique and selected by its mode",
            ));
        }
    }
    Ok(())
}

fn references(value: &ChoiceValue, tag: &str) -> bool {
    value
        .departures
        .as_ref()
        .is_some_and(|rules| rules.iter().any(|rule| rule.as_str() == tag))
}

/// Checks one declared choice and returns the building blocks it selects.
fn check_choice(
    document: &MachineDocument,
    choice: &MachineChoice,
    path: &str,
) -> Result<Vec<ChoiceBlock>, Error> {
    let key = format!("choices.{}", choice.name);
    if !(2..=MAX_CHOICE_VALUES).contains(&choice.values.len()) {
        return Err(invalid(path, &key, "offer 2 to 16 values"));
    }
    if let Some(suggested) = &choice.suggested
        && choice.value(suggested).is_none()
    {
        return Err(invalid(
            path,
            &format!("{key}.suggested"),
            "suggest one of the choice's own values",
        ));
    }
    let Some(first) = choice.values.first() else {
        return Err(invalid(path, &key, "offer 2 to 16 values"));
    };
    let blocks = first.blocks();
    let has_unchecked = check_unchecked(choice, path, &key)?;
    if blocks.is_empty() && !has_unchecked {
        return Err(invalid(
            path,
            &key,
            "each value selects at least one building block",
        ));
    }
    for (index, value) in choice.values.iter().enumerate() {
        let value_key = format!("{key}.values.{}", value.value);
        if choice
            .values
            .iter()
            .take(index)
            .any(|earlier| earlier.value == value.value)
        {
            return Err(invalid(path, &value_key, "each value name is offered once"));
        }
        if value.blocks() != blocks {
            return Err(invalid(
                path,
                &value_key,
                "every value of a choice selects the same building blocks, so each value states its whole effect",
            ));
        }
        if let Some(rules) = &value.departures {
            if choice.kind != ChoiceKind::Mode || document.departure_scope.is_none() {
                return Err(invalid(
                    path,
                    &value_key,
                    "departures require a mode choice and an installed departure_scope",
                ));
            }
            for (at, rule) in rules.iter().enumerate() {
                if rules.iter().take(at).any(|earlier| earlier == rule)
                    || !document.choice_departures.iter().any(|d| {
                        d.tag() == rule.as_str()
                            && d.declaration().scope.mode.as_str() == value.value.as_str()
                    })
                {
                    return Err(invalid(
                        path,
                        &value_key,
                        "select each departure once and only from declarations for this mode",
                    ));
                }
            }
        }
        if value.autolyser.is_some() && document.autolyser.is_none() {
            return Err(invalid(
                path,
                &value_key,
                "Autolyser choices require an [autolyser] table",
            ));
        }
        if value.orders == Some(OrdersSwitch::On) && document.orders.is_none() {
            return Err(invalid(
                path,
                &format!("{value_key}.orders"),
                "orders = \"on\" keeps the definition's [orders]; write [orders] in the definition",
            ));
        }
    }
    if blocks.iter().any(|block| {
        matches!(
            block,
            ChoiceBlock::VidasAlternate | ChoiceBlock::VidasSampleLimit
        )
    }) && document.vidas.is_none()
    {
        return Err(invalid(
            path,
            &key,
            "a vidas choice changes the definition's [vidas]; write [vidas] in the definition",
        ));
    }
    Ok(blocks)
}

/// An unchecked version cannot supply a setting that was inferred from a manual.
fn check_unchecked(choice: &MachineChoice, path: &str, key: &str) -> Result<bool, Error> {
    let mut unchecked = choice
        .values
        .iter()
        .filter(|value| value.version_evidence == VersionEvidence::Unchecked);
    let Some(value) = unchecked.next() else {
        return Ok(false);
    };
    let value_key = format!("{key}.values.{}.checked", value.value);
    if choice.kind != ChoiceKind::SoftwareVersion || unchecked.next().is_some() {
        return Err(invalid(
            path,
            &value_key,
            "only a software_version choice may offer checked = false, on at most one value",
        ));
    }
    if choice.suggested.as_ref() == Some(&value.value) {
        return Err(invalid(
            path,
            &format!("{key}.suggested"),
            "select the unchecked version explicitly in the site file",
        ));
    }
    if choice.values.iter().any(|value| !value.blocks().is_empty()) {
        return Err(invalid(
            path,
            key,
            "a choice offering an unchecked version cannot select settings. Use a separate explicit mode choice for settings that depend on the version",
        ));
    }
    Ok(true)
}

/// Validates the definition with every combination of its choices applied,
/// the lab's `selected` one first so its refusal is the one shown; without
/// choices, the definition alone. A serial value the definition leaves out is
/// not a fault in it when the overrides replace the whole transport.
pub(crate) fn validate_offered(
    document: &MachineDocument,
    selected: &[usize],
    transport_overridden: bool,
    [machine_path, site_path]: [&str; 2],
) -> Result<(), Error> {
    for (at, combination) in combinations(&document.choices, selected).iter().enumerate() {
        let applied = applied(document, combination);
        match validate_machine(&applied, machine_path, machine_path) {
            Err(Error::MissingSerialSetting { .. }) if transport_overridden => {}
            // A cable value no layer gives is the lab's to choose (ADR 0031),
            // whichever combination finds it first.
            Err(missing @ Error::MissingSerialSetting { .. }) => return Err(missing),
            Err(source) if !document.choices.is_empty() => {
                return Err(Error::ChoiceCombination {
                    path: if at == 0 { site_path } else { machine_path }.into(),
                    combination: describe(&document.choices, combination),
                    source: Box::new(source),
                });
            }
            checked => {
                checked?;
            }
        }
    }
    Ok(())
}

/// The value index of each declared choice: the site's selection, else the
/// suggestion. `chosen` says which ones the site selected.
pub(crate) fn resolve(
    choices: &[MachineChoice],
    selections: &ChoiceSelections,
    site_path: &str,
) -> Result<Vec<(usize, bool)>, Error> {
    if let Some(unknown) = selections
        .keys()
        .find(|name| !choices.iter().any(|choice| &choice.name == *name))
    {
        return Err(Error::UnknownChoice {
            path: site_path.into(),
            choice: unknown.clone(),
            declared: if choices.is_empty() {
                "none".into()
            } else {
                choices
                    .iter()
                    .map(|choice| choice.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        });
    }
    choices
        .iter()
        .map(|choice| {
            let (wanted, selected) = match (selections.get(&choice.name), &choice.suggested) {
                (Some(selection), _) => (selection, true),
                (None, Some(suggested)) => (suggested, false),
                (None, None) => {
                    return Err(Error::MissingChoice {
                        path: site_path.into(),
                        choice: choice.name.clone(),
                        allowed: choice.allowed(),
                    });
                }
            };
            let index = choice
                .values
                .iter()
                .position(|value| &value.value == wanted)
                .ok_or_else(|| Error::ChoiceValue {
                    path: site_path.into(),
                    choice: choice.name.clone(),
                    value: wanted.clone(),
                    allowed: choice.allowed(),
                })?;
            Ok((index, selected))
        })
        .collect()
}

/// Every combination of value indexes, the first being `first`; the count is
/// bounded by [`check`].
pub(crate) fn combinations(choices: &[MachineChoice], first: &[usize]) -> Vec<Vec<usize>> {
    let mut all: Vec<Vec<usize>> = vec![Vec::new()];
    for choice in choices {
        all = all
            .into_iter()
            .flat_map(|prefix| {
                (0..choice.values.len()).map(move |index| {
                    let mut next = prefix.clone();
                    next.push(index);
                    next
                })
            })
            .collect();
    }
    all.retain(|combination| combination.as_slice() != first);
    all.insert(0, first.to_vec());
    all
}

/// `name=value` for each choice of `combination`, for an error.
pub(crate) fn describe(choices: &[MachineChoice], combination: &[usize]) -> String {
    choices
        .iter()
        .zip(combination)
        .filter_map(|(choice, index)| {
            choice
                .values
                .get(*index)
                .map(|value| format!("{}={}", choice.name, value.value))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The definition with the values of `combination` applied and its
/// declarations removed: what runs.
pub(crate) fn applied(document: &MachineDocument, combination: &[usize]) -> MachineDocument {
    let mut applied = document.clone();
    let choices = std::mem::take(&mut applied.choices);
    for (choice, index) in choices.iter().zip(combination) {
        if let Some(value) = choice.values.get(*index) {
            apply(&mut applied, value);
        }
    }
    applied.choice_departures.clear();
    applied
}

fn apply(document: &mut MachineDocument, value: &ChoiceValue) {
    if let (Some(_), Some(scope)) = (&value.departures, &mut document.departure_scope) {
        // An offered mode is an installed fact. The remaining scope comes
        // from the definition and must match the declaration during validation.
        if let Ok(mode) = analyte_model::Identifier::new(value.value.as_str()) {
            scope.mode = mode;
        }
        document.deviations.extend(
            document
                .choice_departures
                .iter()
                .filter(|d| {
                    d.declaration().scope.mode.as_str() == value.value.as_str()
                        && references(value, d.tag())
                })
                .cloned(),
        );
    }
    if let Some(encoding) = value.encoding {
        document.syntax.encoding = encoding;
    }
    if let (Some(choice), Some(settings)) = (&value.autolyser, &mut document.autolyser) {
        if let Some(controls) = choice.controls {
            settings.controls = controls;
        }
        if let Some(recover) = choice.recover_last_on_poll {
            settings.recover_last_on_poll = recover;
        }
        if let Some(download) = choice.download {
            settings.download = download;
            // This mode disables transmission while preserving and validating
            // the order layout for modes that can send work lists.
            if !download && let Some(orders) = &mut document.orders {
                orders.send_ahead = false;
            }
        }
    }
    if let Some(mappings) = &value.mappings {
        document.mappings = Some(mappings.clone());
    }
    match value.orders {
        Some(OrdersSwitch::Off) => document.orders = None,
        Some(OrdersSwitch::On) | None => {}
    }
    if let (Some(choice), Some(vidas)) = (&value.vidas, document.vidas.as_mut()) {
        if let Some(alternate) = choice.alternate {
            vidas.ack_point = alternate;
        }
        if let Some(limit) = choice.sample_limit {
            vidas.sample_limit = limit;
        }
    }
}

/// The resolved choices for the effective snapshot and the pages.
pub(crate) fn chosen(choices: &[MachineChoice], resolved: &[(usize, bool)]) -> Vec<Chosen> {
    choices
        .iter()
        .zip(resolved)
        .filter_map(|(choice, (index, selected))| {
            let value = choice.values.get(*index)?;
            Some(Chosen {
                name: choice.name.clone(),
                kind: choice.kind,
                label: choice.label.clone(),
                value: value.value.clone(),
                value_label: value.label.clone(),
                version_evidence: value.version_evidence,
                suggested: choice.suggested.as_ref().and_then(|suggested| {
                    choice
                        .value(suggested)
                        .map(|value| (suggested.clone(), value.label.clone()))
                }),
                selected: *selected,
            })
        })
        .collect()
}

/// A block one choice owns that the overrides file also replaces; keeping
/// both would make two places decide it.
pub(crate) fn refuse_overrides(
    choices: &[MachineChoice],
    overrides: &crate::Overrides,
    path: &str,
) -> Result<(), Error> {
    for choice in choices {
        let Some(first) = choice.values.first() else {
            continue;
        };
        for block in first.blocks() {
            let replaced = match block {
                ChoiceBlock::Mappings => overrides.mappings.is_some(),
                ChoiceBlock::Orders => overrides.orders.is_some(),
                ChoiceBlock::Departures => {
                    overrides.deviations.is_some() || overrides.departure_scope.is_some()
                }
                ChoiceBlock::Encoding => overrides.syntax.is_some(),
                ChoiceBlock::AutolyserControls
                | ChoiceBlock::AutolyserRecovery
                | ChoiceBlock::AutolyserDownload
                | ChoiceBlock::VidasAlternate
                | ChoiceBlock::VidasSampleLimit => false,
            };
            if replaced {
                return Err(Error::ChoiceOverridden {
                    path: path.into(),
                    choice: choice.name.clone(),
                    block,
                });
            }
        }
    }
    Ok(())
}
