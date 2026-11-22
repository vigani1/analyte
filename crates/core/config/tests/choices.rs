//! Tests the choices a machine definition declares and the lab's selections in the site file (ADR 0032).
use analyte_config::{
    AckPoint, ChoiceBlock, ChoiceKind, Draft, Error, RecordTransport, Stack, Validated, Vidas,
    VidasSampleLimit, declared_choices,
};
use analyte_fixtures::ConfigEdit as _;
const VIDAS: &str = include_str!("../examples/vidas_choices.toml");
const ASTM: &str = include_str!("../examples/astm.toml");
const ORDERS: &str = include_str!("../examples/orders.toml");
const SITE: &str = include_str!("../examples/site.toml");
const OVERRIDES: &str = "schema_version = 1";
/// The version band, which the example leaves for the lab to choose.
const V1: &str = "[choices]\nsoftware_version = \"v1\"\n";

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn load(machine: &str, site_extra: &str, overrides: &str) -> Result<Validated, Error> {
    Draft::load(
        ("machine.toml", machine),
        ("site.toml", &format!("{SITE}{site_extra}")),
        ("local.toml", overrides),
    )?
    .validate()
}

fn vidas(config: &Validated) -> Result<&Vidas, &'static str> {
    match config.effective().stack() {
        Stack::Vidas(stack) => Ok(stack.policy()),
        Stack::Astm(_)
        | Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Autolyser(_)
        | Stack::Record(_) => Err("not a VIDAS stack"),
    }
}

/// The ASTM teaching definition with orders and a "send results only" mode.
fn results_only() -> String {
    format!(
        "{ASTM}\n{ORDERS}\n{}",
        r#"
[[choices]]
name = "results_only"
kind = "mode"
label = "Send results only"
suggested = "no"
[[choices.values]]
value = "no"
label = "No: the analyzer asks for its orders"
orders = "on"
[[choices.values]]
value = "yes"
label = "Yes: results only"
orders = "off"
"#
    )
}

// ADR 0032 decisions 1 and 3: declared choices load; a choice without a
// selection takes the suggestion, and the effective config says which.
#[test]
fn declared_choices_load_with_the_suggestion_or_the_selection() -> TestResult {
    let declared = declared_choices("machine.toml", VIDAS)?;
    assert_eq!(
        declared
            .iter()
            .map(|choice| (choice.name.as_str(), choice.kind))
            .collect::<Vec<_>>(),
        [
            ("protocol_mode", ChoiceKind::Mode),
            ("software_version", ChoiceKind::SoftwareVersion)
        ]
    );
    let config = load(VIDAS, V1, OVERRIDES)?;
    let chosen = config.effective().choices();
    assert_eq!(chosen.len(), 2);
    let (mode, version) = (&chosen[0], &chosen[1]);
    assert_eq!(
        (mode.value.as_str(), mode.selected, mode.differs()),
        ("literal", false, false)
    );
    assert_eq!(
        (version.value.as_str(), version.selected, version.differs()),
        ("v1", true, true),
        "a choice the definition suggests nothing for always differs from the suggestion"
    );
    assert_eq!(vidas(&config)?.ack_point, AckPoint::Checksum);
    Ok(())
}

// ADR 0032 decision 3: no selection and no suggestion is refused by name; nothing is assumed.
#[test]
fn a_choice_without_selection_or_suggestion_is_refused_by_name() {
    let refused = load(VIDAS, "", OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::MissingChoice { path, choice, allowed })
            if path == "site.toml" && choice.as_str() == "software_version" && allowed == "v1, v2"),
        "{refused:?}"
    );
    let words = refused
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        words.contains("--choice software_version=<value>"),
        "{words}"
    );
}

// ADR 0032 decision 3: an unknown choice or a value off the declared list is refused.
#[test]
fn unknown_choices_and_values_are_refused() {
    let unknown = load(
        VIDAS,
        "[choices]\nsoftware_version = \"v1\"\nspeed = \"fast\"\n",
        OVERRIDES,
    );
    assert!(
        matches!(&unknown, Err(Error::UnknownChoice { choice, declared, .. })
            if choice.as_str() == "speed" && declared == "protocol_mode, software_version"),
        "{unknown:?}"
    );
    let value = load(VIDAS, "[choices]\nsoftware_version = \"v3\"\n", OVERRIDES);
    assert!(
        matches!(&value, Err(Error::ChoiceValue { choice, value, .. })
            if choice.as_str() == "software_version" && value.as_str() == "v3"),
        "{value:?}"
    );
    let none_declared = load(ASTM, V1, OVERRIDES);
    assert!(
        matches!(&none_declared, Err(Error::UnknownChoice { declared, .. }) if declared == "none"),
        "{none_declared:?}"
    );
    let spelled = load(VIDAS, "[choices]\nsoftware_version = \"V1\"\n", OVERRIDES);
    assert!(matches!(&spelled, Err(Error::Schema { .. })), "{spelled:?}");
}

// ADR 0032 decision 4: a selection changes the effective stack and the
// digest; the same values give the same bytes whether chosen or suggested.
#[test]
fn a_selection_changes_the_stack_and_the_digest() -> TestResult {
    let suggested = load(VIDAS, V1, OVERRIDES)?;
    let explicit = load(
        VIDAS,
        "[choices]\nsoftware_version = \"v1\"\nprotocol_mode = \"literal\"\n",
        OVERRIDES,
    )?;
    assert_eq!(suggested.canonical(), explicit.canonical());
    let alternate = load(
        VIDAS,
        "[choices]\nsoftware_version = \"v1\"\nprotocol_mode = \"alternate\"\n",
        OVERRIDES,
    )?;
    assert_eq!(vidas(&alternate)?.ack_point, AckPoint::Etx);
    assert_ne!(suggested.digest(), alternate.digest());
    assert!(
        alternate
            .canonical()
            .contains("protocol_mode = \"alternate\"")
    );
    assert!(alternate.canonical().contains("alternate = true"));
    assert!(
        !alternate.canonical().contains("[[machine.choices]]"),
        "the snapshot holds what runs, not the alternatives"
    );
    assert!(alternate.effective().choices()[0].differs());
    Ok(())
}

// ADR 0032 decision 2; VIDAS BCI Link rev A chapter 4: the version band, not
// a free value, decides the sample ID limit.
#[test]
fn the_version_band_picks_the_version_fact() -> TestResult {
    for (band, limit) in [
        ("v1", VidasSampleLimit::Twelve),
        ("v2", VidasSampleLimit::Seventeen),
    ] {
        let config = load(
            VIDAS,
            &format!("[choices]\nsoftware_version = \"{band}\"\n"),
            OVERRIDES,
        )?;
        assert_eq!(vidas(&config)?.sample_limit, limit, "{band}");
    }
    Ok(())
}

// ADR 0032 decision 1: a mode can turn the definition's orders off ("send
// results only"); it never writes order settings of its own.
#[test]
fn a_results_only_mode_turns_the_orders_off() -> TestResult {
    let orders = |config: &Validated| match config.effective().stack() {
        Stack::Astm(RecordTransport::Stream(stack)) => Ok(stack.orders().is_some()),
        Stack::Astm(RecordTransport::Folder(_))
        | Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Vidas(_)
        | Stack::Autolyser(_)
        | Stack::Record(_) => Err("not an ASTM stream stack"),
    };
    assert!(orders(&load(&results_only(), "", OVERRIDES)?)?);
    let off = load(
        &results_only(),
        "[choices]\nresults_only = \"yes\"\n",
        OVERRIDES,
    )?;
    assert!(!orders(&off)?);
    let without_orders = format!(
        "{ASTM}\n{}",
        results_only()
            .split_once(ORDERS)
            .map_or("", |(_, rest)| rest)
    );
    let refused = load(&without_orders, "", OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "choices.results_only.values.no.orders"),
        "{refused:?}"
    );
    Ok(())
}

// ADR 0032 decision 5; ADR 0015: a choice cannot change evidence or departures.
#[test]
fn a_choice_cannot_change_evidence_or_departures() {
    for (key, text) in [
        ("choices.deviations", "deviations = []"),
        (
            "choices.evidence",
            "evidence = {level = \"proven\", capabilities = []}",
        ),
        ("choices.departure_scope", "departure_scope = {}"),
    ] {
        let machine = VIDAS.rewrite(
            "vidas = {sample_limit = 17}",
            &format!("vidas = {{sample_limit = 17}}\n{text}"),
        );
        let refused = load(&machine, V1, OVERRIDES);
        assert!(
            matches!(&refused, Err(Error::Invalid { key: found, .. }) if found == key),
            "{key}: {refused:?}"
        );
    }
}

// ADR 0032 decision 1: a setting a choice selects is written in one place,
// the choice; the definition's own value beside it is refused.
#[test]
fn a_chosen_setting_is_written_only_in_the_choice() {
    let machine = VIDAS.rewrite("chunk = 80", "chunk = 80\nsample_limit = 12");
    let refused = load(&machine, V1, OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "vidas.sample_limit"),
        "{refused:?}"
    );
    let without_vidas = VIDAS
        .split_once("\n[vidas]\n")
        .zip(VIDAS.split_once("\n[codes]\n"))
        .map(|((head, _), (_, tail))| format!("{head}\n[codes]\n{tail}"))
        .unwrap_or_default();
    let refused = load(&without_vidas, V1, OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "choices.protocol_mode"),
        "{refused:?}"
    );
}

// ADR 0032 decision 1: declarations are a closed, checked list.
#[test]
fn declarations_break_no_choice_rule() {
    let overlap = VIDAS
        .rewrite(
            "vidas = {sample_limit = 12}",
            "vidas = {sample_limit = 12, alternate = false}",
        )
        .rewrite(
            "vidas = {sample_limit = 17}",
            "vidas = {sample_limit = 17, alternate = true}",
        );
    let refused = load(&overlap, V1, OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::ChoiceOverlap { first, second, block: ChoiceBlock::VidasAlternate, .. })
            if first.as_str() == "protocol_mode" && second.as_str() == "software_version"),
        "{refused:?}"
    );
    for (from, to, key) in [
        (
            "suggested = \"literal\"",
            "suggested = \"fast\"",
            "choices.protocol_mode.suggested",
        ),
        (
            "vidas = {alternate = true}",
            "vidas = {sample_limit = 17}",
            "choices.protocol_mode.values.alternate",
        ),
        (
            "value = \"alternate\"",
            "value = \"literal\"",
            "choices.protocol_mode.values.literal",
        ),
        (
            "name = \"software_version\"",
            "name = \"protocol_mode\"",
            "choices.protocol_mode",
        ),
        ("kind = \"mode\"", "kind = \"software_version\"", "choices"),
    ] {
        let refused = load(&VIDAS.rewrite(from, to), V1, OVERRIDES);
        assert!(
            matches!(&refused, Err(Error::Invalid { key: found, .. }) if found == key),
            "{to}: {refused:?}"
        );
    }
    let single = VIDAS.rewrite(
        "[[choices.values]]\nvalue = \"alternate\"\nlabel = \"Alternate (acknowledge after ETX)\"\nvidas = {alternate = true}\n",
        "",
    );
    let refused = load(&single, V1, OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "choices.protocol_mode"),
        "{refused:?}"
    );
    let empty_value = VIDAS.rewrite("vidas = {alternate = true}", "");
    assert!(load(&empty_value, V1, OVERRIDES).is_err());
}

// ADR 0032 decision 1: every combination the definition offers must run; the
// lab's own combination is refused with its site file, another with the definition.
#[test]
fn a_combination_the_stack_refuses_is_refused() {
    // Sending orders ahead to a VIDAS needs the definition's own download
    // permission, which this definition withholds.
    let machine = format!(
        "{VIDAS}\n{}",
        r#"
[orders]
send_ahead = true
patient_namespace = "synthetic-patients"
priorities = [{app="routine",machine="R"}]
[[choices]]
name = "work_list"
kind = "mode"
label = "Work list download"
suggested = "off"
[[choices.values]]
value = "on"
label = "On"
orders = "on"
[[choices.values]]
value = "off"
label = "Off"
orders = "off"
"#
    );
    let refused = load(&machine, V1, OVERRIDES);
    let Err(Error::ChoiceCombination {
        path, combination, ..
    }) = &refused
    else {
        panic!("a combination the stack refuses must be refused: {refused:?}");
    };
    assert_eq!(path, "machine.toml");
    assert!(combination.contains("work_list=on"), "{combination}");
    let own = load(
        &machine,
        "[choices]\nsoftware_version = \"v1\"\nwork_list = \"on\"\n",
        OVERRIDES,
    );
    assert!(
        matches!(&own, Err(Error::ChoiceCombination { path, .. }) if path == "site.toml"),
        "{own:?}"
    );
}

// ADR 0032 decision 1: an overrides file cannot replace a block a choice owns.
#[test]
fn overrides_cannot_replace_a_chosen_block() {
    let overrides = format!("schema_version = 1\n{ORDERS}");
    let refused = load(&results_only(), "", &overrides);
    assert!(
        matches!(&refused, Err(Error::ChoiceOverridden { choice, block: ChoiceBlock::Orders, .. })
            if choice.as_str() == "results_only"),
        "{refused:?}"
    );
}
