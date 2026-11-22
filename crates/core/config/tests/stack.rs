//! Tests validated protocol stack construction.
use analyte_config::{Draft, Stack, StreamKind, TcpRole};
use analyte_fixtures::ConfigEdit as _;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const SITE: &str = include_str!("../examples/site.toml");
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");

fn load(
    machine: &str,
    site: &str,
    overrides: &str,
) -> Result<analyte_config::Validated, analyte_config::Error> {
    Draft::load(
        ("machine", machine),
        ("site", site),
        ("overrides", overrides),
    )?
    .validate()
}

fn variant(stack: &Stack) -> &str {
    match stack {
        Stack::Astm(analyte_config::RecordTransport::Stream(_)) => "astm",
        Stack::Hl7(analyte_config::RecordTransport::Stream(_)) => "hl7",
        Stack::Law(_) => "law",
        Stack::Poct(_) => "poct",
        Stack::Vidas(_) => "vidas",
        Stack::Autolyser(_) => "autolyser",
        Stack::Astm(analyte_config::RecordTransport::Folder(_)) => "file_astm",
        Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => "file_hl7",
    }
}

/// The lab apps example with the example site file naming its lab app gives
/// the same stack and the same snapshot as the site file's own [destination].
fn assert_named_lab_app(base: &str, text: &str, expected: &str) -> TestResult {
    let at = SITE.find("[destination]").ok_or("example site")?;
    let named = format!("{}lab_app = \"demo\"\n", SITE.get(..at).ok_or("site")?);
    let lab_apps = analyte_config::LabApps::load("lab-apps.toml", text)?;
    let validated = Draft::load_with(
        ("machine", base),
        ("site", &named),
        ("overrides", "schema_version=1"),
        &lab_apps,
    )?
    .validate()?;
    assert_eq!(variant(validated.effective().stack()), expected);
    let embedded = load(base, SITE, "schema_version=1")?;
    assert_eq!(validated.canonical(), embedded.canonical());
    Ok(())
}

// ADR 0002
#[test]
fn every_example_maps_to_its_documented_stack() -> TestResult {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    // Include fragments with their documented base, and the site with a receiver.
    // Comparing the directory inventory makes adding an untested example fail.
    let cases = [
        ("astm.toml", "", "astm"),
        ("hl7.toml", "", "hl7"),
        ("law.toml", "", "law"),
        ("poct.toml", "", "poct"),
        ("vidas_literal.toml", "", "vidas"),
        // Its software version has no suggestion; the site below chooses it.
        ("vidas_choices.toml", "", "vidas"),
        ("autolyser.toml", "", "autolyser"),
        ("orders.toml", ASTM, "astm"),
        ("hl7_orders.toml", HL7, "hl7"),
        ("hl7_send.toml", HL7, "hl7"),
        (
            "law_orders.toml",
            include_str!("../examples/law.toml"),
            "law",
        ),
        ("site.toml", HL7, "hl7"),
        // The lab apps file, with the example site file naming its lab app.
        ("lab-apps.toml", HL7, "hl7"),
    ];
    let mut found = std::fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut expected = cases
        .iter()
        .map(|(name, _, _)| (*name).to_owned())
        .collect::<Vec<_>>();
    found.sort();
    expected.sort();
    assert_eq!(found, expected, "every example needs a stack assertion");
    for (name, base, expected) in cases {
        let text = std::fs::read_to_string(directory.join(name))?;
        if name == "lab-apps.toml" {
            assert_named_lab_app(base, &text, expected)?;
            continue;
        }
        let (machine, site) = match name {
            "site.toml" => (base.to_owned(), text),
            "vidas_choices.toml" => (
                text,
                format!("{SITE}[choices]\nsoftware_version = \"v1\"\n"),
            ),
            _ => (format!("{base}\n{text}"), SITE.to_owned()),
        };
        let validated = load(&machine, &site, "schema_version=1")?;
        assert_eq!(variant(validated.effective().stack()), expected, "{name}");
        let canonical: toml::Value = toml::from_str(validated.canonical())?;
        assert!(canonical.get("stack").is_none());
        assert_eq!(toml::Value::try_from(validated.effective())?, canonical);
        let original: toml::Value = toml::from_str(&machine)?;
        let raw = validated.effective().machine();
        match validated.effective().stack() {
            Stack::Astm(analyte_config::RecordTransport::Stream(stack)) => {
                assert_astm_example(stack, raw, name, &original);
            }
            Stack::Hl7(analyte_config::RecordTransport::Stream(stack)) => {
                assert_hl7_example(stack, raw, name);
            }
            Stack::Law(stack) => {
                assert_eq!(stack.settings().transport().role(), TcpRole::Server);
                assert_eq!(stack.policy().query_deadline_ms(), 1500);
                assert_eq!(stack.orders().is_some(), name == "law_orders.toml");
                if let Some(orders) = &stack.orders() {
                    assert_eq!(
                        orders
                            .tables()
                            .specimen_system()
                            .map(analyte_model::Identifier::as_str),
                        Some("HL70487")
                    );
                    assert_eq!(orders.work().application.as_str(), "HOST");
                }
            }
            Stack::Poct(stack) => {
                assert_eq!(stack.policy().depth, 32);
                assert_eq!(
                    stack.settings().codes().test_system.as_str(),
                    "synthetic-system"
                );
            }
            Stack::Vidas(stack) => {
                assert_eq!(u8::from(stack.policy().sample_limit), 12);
                assert_eq!(stack.settings().codes().test_system.as_str(), "synthetic");
                assert!(stack.orders().is_none());
            }
            Stack::Autolyser(stack) => {
                assert_eq!(stack.policy().poll_ms, 1000);
                assert_eq!(stack.settings().codes().test_system.as_str(), "synthetic");
                assert!(stack.orders().is_none());
            }
            Stack::Astm(analyte_config::RecordTransport::Folder(_))
            | Stack::Hl7(analyte_config::RecordTransport::Folder(_)) => {
                panic!("no folder example is shipped")
            }
        }
    }
    Ok(())
}

// ADR 0002
#[test]
fn stack_built_after_overrides_keeps_serial_settings() -> TestResult {
    let original = load(ASTM, SITE, "schema_version=1")?;
    let Stack::Astm(analyte_config::RecordTransport::Stream(stack)) = original.effective().stack()
    else {
        panic!("ASTM stack")
    };
    let StreamKind::Serial(settings) = stack.settings().transport().kind() else {
        panic!("serial transport")
    };
    let operation_timeout_ms = stack.settings().transport().operation_timeout_ms();
    assert_eq!(settings.baud, 9600);
    assert_eq!(operation_timeout_ms, 10000);
    let overridden = load(
        ASTM,
        SITE,
        r#"schema_version=1
[transport]
kind="tcp_client"
operation_timeout_ms=1234
"#,
    )?;
    let Stack::Astm(analyte_config::RecordTransport::Stream(stack)) =
        overridden.effective().stack()
    else {
        panic!("ASTM stack")
    };
    assert!(
        matches!(
            stack.settings().transport().kind(),
            StreamKind::Tcp(TcpRole::Client)
        ) && stack.settings().transport().operation_timeout_ms() == 1234
    );
    Ok(())
}

// ADR 0002
#[test]
fn file_pickup_has_a_stack_per_record_syntax() -> TestResult {
    let endpoint = std::env::current_dir()?.to_string_lossy().into_owned();
    let site = SITE.rewrite(
        r#"endpoint = "127.0.0.1:5600""#,
        &format!("endpoint = {endpoint:?}"),
    );
    for (definition, expected) in [(ASTM, "file_astm"), (HL7, "file_hl7")] {
        let start = definition.find("[transport]").ok_or("transport section")?;
        let end = definition.find("[syntax]").ok_or("syntax section")?;
        let machine = format!(
            "{}{}{}",
            &definition[..start],
            r#"[transport]
kind="watched_folder"
operation_timeout_ms=10000
[framing]
limit=1048576
[dialog]
receive_timeout_ms=30000
send_timeout_ms=15000
"#,
            &definition[end..]
        );
        let validated = load(&machine, &site, "schema_version=1")?;
        assert_eq!(variant(validated.effective().stack()), expected);
    }
    Ok(())
}

// ADR 0012
#[test]
fn invalid_protocol_transport_pairs_fail_loading() -> TestResult {
    for (name, protocol, serial) in [
        ("astm", "astm", true),
        ("hl7", "hl7", false),
        ("law", "law", false),
        ("poct", "poct", false),
        ("vidas_literal", "vidas", true),
        ("autolyser", "autolyser", true),
    ] {
        let definition = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("examples/{name}.toml")),
        )?;
        for transport in ["serial", "tcp_client", "tcp_server", "watched_folder"] {
            let serial_settings = if transport == "serial" {
                "[transport.serial]\nbaud=9600\ndata_bits=8\nparity='none'\nstop_bits=1\nflow_control='none'\n"
            } else {
                ""
            };
            let overrides = format!(
                "schema_version=1\n[transport]\nkind='{transport}'\noperation_timeout_ms=10000\n{serial_settings}"
            );
            let result = load(&definition, SITE, &overrides);
            let allowed = matches!(transport, "tcp_client" | "tcp_server")
                || (serial && transport == "serial");
            if allowed {
                assert!(result.is_ok(), "{protocol}/{transport}: {result:?}");
            } else if matches!(protocol, "astm" | "hl7") && transport == "watched_folder" {
                assert!(
                    matches!(result, Err(analyte_config::Error::Invalid { key, .. }) if key == "dialog")
                );
            } else {
                assert!(
                    matches!(result, Err(analyte_config::Error::IncompatibleStack { .. })),
                    "{protocol}/{transport}: {result:?}"
                );
            }
        }
        let wrong = definition.rewrite(
            &format!(r#"protocol = "{protocol}""#),
            r#"protocol = "unknown""#,
        );
        {
            let Err(refusal) = load(&wrong, SITE, "schema_version=1") else {
                panic!("input must be refused")
            };
            assert!(
                matches!(&refusal, analyte_config::Error::Schema { .. }),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
    Ok(())
}

fn assert_astm_example(
    stack: &analyte_config::AstmStack,
    raw: &analyte_config::Machine,
    name: &str,
    original: &toml::Value,
) {
    assert_eq!(
        Some(stack.settings().frame_limit()),
        original
            .get("framing")
            .and_then(|f| f.get("limit"))
            .and_then(toml::Value::as_integer)
            .and_then(|n| usize::try_from(n).ok())
    );
    assert_eq!(&stack.settings().codes().test_system, raw.test_system());
    assert!(matches!(
        stack.settings().transport().kind(),
        StreamKind::Serial(_)
    ));
    assert_eq!(stack.orders().is_some(), name == "orders.toml");
    if let Some(orders) = &stack.orders() {
        assert!(orders.query().is_some());
        assert_eq!(orders.layout().test.field, 5);
    }
}

fn assert_hl7_example(stack: &analyte_config::Hl7Stack, raw: &analyte_config::Machine, name: &str) {
    assert_eq!(stack.settings().transport().role(), TcpRole::Server);
    assert_eq!(&stack.settings().codes().test_system, raw.test_system());
    assert_eq!(
        stack.orders().is_some(),
        matches!(name, "hl7_orders.toml" | "hl7_send.toml")
    );
    if let Some(orders) = &stack.orders() {
        assert_eq!(orders.query().is_some(), name == "hl7_orders.toml");
        assert_eq!(orders.send().is_some(), name == "hl7_send.toml");
        if let Some(send) = &orders.send() {
            assert_eq!(send.placer_namespace().as_str(), "LAB");
        }
    }
}
