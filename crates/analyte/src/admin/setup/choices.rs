//! The choices a machine model declares that Setup asks (ADR 0032): read from the definition, the draft and the saved site file, and the words the pages and commands show.
//!
//! The definition declares each choice and its values; Setup step 2 shows one
//! list per choice, filled in with the draft's value, the saved one or the
//! manual's suggestion, and Save writes every value into the site file's
//! `[choices]`. The config loader checks them, as it checks every layer.
use super::SetupError;
use super::wizard::Draft;
use crate::machine::software_version::unchecked_warning;
use analyte_config::{
    ChoiceKind, ChoiceLabel, ChoiceName, ChoiceSelections, Chosen, MachineChoice, VersionEvidence,
};
use analyte_i18n::{message as tr_args, text as tr};

/// The prefix of a choice's form field: `choice.<name>`.
pub(in crate::admin::setup) const FIELD: &str = "choice.";

/// The values a site file chooses, as written; none when it writes none.
pub(in crate::admin::setup) fn saved(site: &toml::Table) -> Vec<(String, String)> {
    site.get("choices")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// The draft's typed value of the choice `name`, trimmed; none when empty.
pub(in crate::admin::setup) fn typed<'a>(draft: &'a Draft, name: &str) -> Option<&'a str> {
    draft
        .choices
        .get(name)
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
}

/// Fills each choice `declared` that `draft` leaves empty from the saved site
/// file, so a saved choice is kept unless changed.
pub(in crate::admin::setup) fn keep_saved(
    draft: &mut Draft,
    site: &toml::Table,
    declared: &[MachineChoice],
) {
    for (name, value) in saved(site) {
        let known = declared.iter().any(|choice| choice.name.as_str() == name);
        if known && typed(draft, &name).is_none() {
            draft.choices.insert(name, value);
        }
    }
}

/// The value of each choice `declared`: the draft's, else the suggestion;
/// a choice with neither is left out, so the loader refuses it by name. A
/// typed choice the model does not declare, or a value it does not offer,
/// is refused here.
pub(in crate::admin::setup) fn chosen(
    draft: &Draft,
    declared: &[MachineChoice],
) -> Result<ChoiceSelections, SetupError> {
    if let Some(unknown) = draft
        .choices
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(name, _)| name)
        .find(|name| !declared.iter().any(|choice| choice.name.as_str() == *name))
    {
        return Err(SetupError::UnknownChoice(unknown.clone()));
    }
    let mut chosen = ChoiceSelections::new();
    for choice in declared {
        let value = match typed(draft, choice.name.as_str()) {
            Some(text) => Some(
                ChoiceName::new(text)
                    .ok()
                    .filter(|value| choice.value(value).is_some())
                    .ok_or_else(|| SetupError::ChoiceValue {
                        choice: choice.name.to_string(),
                        allowed: choice.allowed(),
                    })?,
            ),
            None => choice.suggested.clone(),
        };
        if let Some(value) = value {
            chosen.insert(choice.name.clone(), value);
        }
    }
    Ok(chosen)
}

/// The choices `declared` that have neither a draft value nor a suggestion.
pub(in crate::admin::setup) fn missing<'a>(
    draft: &Draft,
    declared: &'a [MachineChoice],
) -> Vec<&'a MachineChoice> {
    declared
        .iter()
        .filter(|choice| typed(draft, choice.name.as_str()).is_none() && choice.suggested.is_none())
        .collect()
}

/// Where on the analyzer the lab reads this choice.
pub(in crate::admin::setup) fn hint(kind: ChoiceKind) -> &'static str {
    match kind {
        ChoiceKind::Mode => tr("setup-match-the-setting-on-the-analyzer-s-own-screen"),
        ChoiceKind::SoftwareVersion => tr("setup-read-the-version-on-the-analyzer-s-about-screen"),
    }
}

/// The displayed value, with the warning when it is outside the manuals.
pub(in crate::admin::setup) fn value_words(
    label: &ChoiceLabel,
    evidence: VersionEvidence,
) -> String {
    match evidence {
        VersionEvidence::Checked => label.as_str().to_owned(),
        VersionEvidence::Unchecked => tr_args(
            "setup-choice-unchecked",
            [
                ("label", label.as_str().into()),
                ("warning", unchecked_warning().into()),
            ],
        ),
    }
}

/// One choice in words: "Protocol: Alternate (the manual suggests Literal)".
fn word(chosen: &Chosen) -> String {
    let value = value_words(&chosen.value_label, chosen.version_evidence);
    if chosen.version_evidence == VersionEvidence::Unchecked {
        return format!("{}: {value}", chosen.label.as_str());
    }
    match (&chosen.suggested, chosen.differs()) {
        (_, false) => format!("{}: {value}", chosen.label.as_str()),
        (Some((_, suggested)), true) => tr_args(
            "setup-the-manual-suggests-2",
            [
                ("label", (chosen.label.as_str()).to_string().into()),
                ("value", (value).to_string().into()),
                ("suggested", (suggested.as_str()).to_string().into()),
            ],
        ),
        (None, true) => tr_args(
            "setup-the-machine-definition-suggests-none-2",
            [
                ("label", (chosen.label.as_str()).to_string().into()),
                ("value", (value).to_string().into()),
            ],
        ),
    }
}

/// The choices in words and, when some differ from the manual's suggestion,
/// which: one line for the command line, `doctor` and the machine's page;
/// none when the model declares no choice.
pub(in crate::admin) fn summary(chosen: &[Chosen]) -> Option<String> {
    (!chosen.is_empty()).then(|| chosen.iter().map(word).collect::<Vec<_>>().join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Vec<MachineChoice> {
        analyte_config::declared_choices(
            "vidas_choices.toml",
            include_str!("../../../../core/config/examples/vidas_choices.toml"),
        )
        .unwrap_or_default()
    }

    // ADR 0032 decision 3: a typed value wins, the suggestion fills the rest,
    // and a choice with neither is left for the loader to refuse by name.
    #[test]
    fn typed_values_win_over_the_suggestion() -> Result<(), SetupError> {
        let declared = declared();
        assert_eq!(declared.len(), 2);
        let mut draft = Draft::default();
        let picked = chosen(&draft, &declared)?;
        assert_eq!(
            picked
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
            [("protocol_mode", "literal")]
        );
        assert_eq!(
            missing(&draft, &declared)
                .iter()
                .map(|choice| choice.name.as_str())
                .collect::<Vec<_>>(),
            ["software_version"]
        );
        draft
            .choices
            .insert("software_version".into(), " v2 ".into());
        draft
            .choices
            .insert("protocol_mode".into(), "alternate".into());
        let picked = chosen(&draft, &declared)?;
        assert_eq!(
            picked.values().map(ChoiceName::as_str).collect::<Vec<_>>(),
            ["alternate", "v2"]
        );
        Ok(())
    }

    // ADR 0032 decision 3: an undeclared choice or an unoffered value is refused.
    #[test]
    fn unknown_choices_and_values_are_refused() {
        let declared = declared();
        let mut draft = Draft::default();
        draft.choices.insert("speed".into(), "fast".into());
        assert!(matches!(
            chosen(&draft, &declared),
            Err(SetupError::UnknownChoice(name)) if name == "speed"
        ));
        let mut draft = Draft::default();
        draft.choices.insert("software_version".into(), "v3".into());
        assert!(matches!(
            chosen(&draft, &declared),
            Err(SetupError::ChoiceValue { choice, .. }) if choice == "software_version"
        ));
    }
}
