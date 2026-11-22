//! Tests the lab's serial cable choice in the site file over the definition's suggestion (ADR 0031).
use analyte_config::{
    Draft, Error, FlowControl, Parity, RecordTransport, Serial, SerialSetting, Stack, StreamKind,
    Validated,
};
use analyte_fixtures::ConfigEdit as _;
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");
const SITE: &str = include_str!("../examples/site.toml");
const OVERRIDES: &str = "schema_version = 1";
const FULL: &str = "[transport.serial]
baud = 9600
data_bits = 8
parity = \"none\"
stop_bits = 1
flow_control = \"none\"
";

fn load(machine: &str, site_extra: &str, overrides: &str) -> Result<Validated, Error> {
    Draft::load(
        ("machine.toml", machine),
        ("site.toml", &format!("{SITE}{site_extra}")),
        ("local.toml", overrides),
    )?
    .validate()
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The serial settings an ASTM serial stack opens its port with.
fn serial(config: &Validated) -> Result<Serial, &'static str> {
    let Stack::Astm(RecordTransport::Stream(stack)) = config.effective().stack() else {
        return Err("not an ASTM stream stack");
    };
    let StreamKind::Serial(serial) = stack.settings().transport().kind() else {
        return Err("not a serial transport");
    };
    Ok(serial.clone())
}

// ADR 0031 decisions 1 and 2: the definition's values are the suggestion; the
// site's [serial] wins value by value, and the snapshot holds what is used.
#[test]
fn site_choice_wins_over_the_definition_suggestion() -> TestResult {
    let suggested = load(ASTM, "", OVERRIDES)?;
    assert_eq!(
        serial(&suggested)?,
        Serial {
            baud: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow_control: FlowControl::None,
        }
    );
    assert!(
        serial(&suggested)?
            .differs_from(suggested.effective().serial_suggestion())
            .is_empty()
    );
    let chosen = load(
        ASTM,
        "[serial]\nbaud = 19200\nparity = \"even\"\n",
        OVERRIDES,
    )?;
    let used = serial(&chosen)?;
    assert_eq!(
        (used.baud, used.parity, used.data_bits),
        (19200, Parity::Even, 8)
    );
    assert_eq!(
        used.differs_from(chosen.effective().serial_suggestion()),
        [SerialSetting::Baud, SerialSetting::Parity]
    );
    assert_eq!(
        chosen.effective().serial_suggestion().and_then(|s| s.baud),
        Some(9600)
    );
    assert!(
        chosen.canonical().contains("baud = 19200"),
        "{}",
        chosen.canonical()
    );
    assert_ne!(chosen.digest(), suggested.digest());
    // The same values written by the lab give the same snapshot as the
    // suggestion, so saving them in Setup does not change the version.
    let same = load(
        ASTM,
        "[serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n",
        OVERRIDES,
    )?;
    assert_eq!(same.digest(), suggested.digest());
    Ok(())
}

// ADR 0031 decision 3; AGENTS.md "never invents data": a value neither the
// definition nor the lab gives is refused with the setting's name.
#[test]
fn missing_value_is_refused_by_name() -> TestResult {
    let unknown_flow = ASTM.rewrite("flow_control = \"none\"\n", "");
    let refused = load(&unknown_flow, "", OVERRIDES);
    assert!(
        matches!(
            &refused,
            Err(Error::MissingSerialSetting {
                setting: SerialSetting::FlowControl,
                ..
            })
        ),
        "{refused:?}"
    );
    let chosen = load(
        &unknown_flow,
        "[serial]\nflow_control = \"hardware\"\n",
        OVERRIDES,
    )?;
    assert_eq!(serial(&chosen)?.flow_control, FlowControl::Hardware);
    assert_eq!(
        serial(&chosen)?.differs_from(chosen.effective().serial_suggestion()),
        [SerialSetting::FlowControl]
    );
    // A definition may leave the whole table to the lab.
    let none = ASTM.rewrite(FULL, "");
    let refused = load(&none, "", OVERRIDES);
    assert!(
        matches!(
            &refused,
            Err(Error::MissingSerialSetting {
                setting: SerialSetting::Baud,
                ..
            })
        ),
        "{refused:?}"
    );
    load(
        &none,
        "[serial]\nbaud = 9600\ndata_bits = 7\nparity = \"odd\"\nstop_bits = 2\nflow_control = \"software\"\n",
        OVERRIDES,
    )?;
    Ok(())
}

// ADR 0031 decision 4: only the standard list; a value outside it is refused
// with the site key that wrote it.
#[test]
fn value_outside_the_standard_list_is_refused() {
    for (extra, key) in [
        ("[serial]\nbaud = 14400\n", "serial.baud"),
        ("[serial]\nbaud = 0\n", "serial.baud"),
        ("[serial]\ndata_bits = 9\n", "serial.data_bits"),
        ("[serial]\nstop_bits = 3\n", "serial.stop_bits"),
    ] {
        let refused = load(ASTM, extra, OVERRIDES);
        assert!(
            matches!(&refused, Err(Error::Invalid { key: found, .. }) if found == key),
            "{extra}: {refused:?}"
        );
    }
    for extra in [
        "[serial]\nparity = \"mark\"\n",
        "[serial]\nflow_control = \"dtr\"\n",
        "[serial]\nkind = \"tcp_client\"\n",
        "[serial]\noperation_timeout_ms = 5\n",
    ] {
        let refused = load(ASTM, extra, OVERRIDES);
        assert!(
            matches!(&refused, Err(Error::Schema { .. })),
            "{extra}: {refused:?}"
        );
    }
    let refused = load(&ASTM.rewrite("baud = 9600", "baud = 14400"), "", OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "transport.serial.baud"),
        "{refused:?}"
    );
}

// ADR 0031 decision 5: serial values belong to a serial machine, and to one
// place: not beside an overrides file that replaces the whole transport.
#[test]
fn serial_choice_needs_a_serial_machine_in_one_place() -> TestResult {
    let refused = load(HL7, "[serial]\nbaud = 9600\n", OVERRIDES);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, .. }) if key == "serial"),
        "{refused:?}"
    );
    let replaced = format!(
        "schema_version = 1\n[transport]\nkind = \"serial\"\noperation_timeout_ms = 10000\n{}",
        FULL.rewrite("baud = 9600", "baud = 4800")
    );
    let refused = load(ASTM, "[serial]\nbaud = 9600\n", &replaced);
    assert!(
        matches!(&refused, Err(Error::Invalid { key, path, .. }) if key == "transport" && path == "local.toml"),
        "{refused:?}"
    );
    // An overrides file alone still replaces the whole section, as before,
    // even when the definition leaves a value out.
    let overridden = load(
        &ASTM.rewrite("flow_control = \"none\"\n", ""),
        "",
        &replaced,
    )?;
    assert_eq!(serial(&overridden)?.baud, 4800);
    Ok(())
}
