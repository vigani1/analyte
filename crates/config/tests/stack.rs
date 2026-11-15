//! ADR 0001/0002: validated stacks preserve the existing layered TOML interface.
use analyte_config::{Draft, Stack, StreamTransport, TcpRole};

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
        Stack::Astm(_) => "astm",
        Stack::Hl7(_) => "hl7",
        Stack::Law(_) => "law",
        Stack::Poct(_) => "poct",
        Stack::VidasLiteral(_) => "vidas",
        Stack::Autolyser(_) => "autolyser",
        Stack::FileAstm(_) => "file_astm",
        Stack::FileHl7(_) => "file_hl7",
    }
}

#[test]
fn adr0002_every_example_maps_to_its_documented_stack() -> TestResult {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    // Include fragments with their documented base, and the site with a receiver.
    // Comparing the directory inventory makes adding an untested example fail.
    let cases = [
        ("astm.toml", "", "astm"),
        ("hl7.toml", "", "hl7"),
        ("law.toml", "", "law"),
        ("poct.toml", "", "poct"),
        ("vidas_literal.toml", "", "vidas"),
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
        let (machine, site) = if name == "site.toml" {
            (base.to_owned(), text)
        } else {
            (format!("{base}\n{text}"), SITE.to_owned())
        };
        let validated = load(&machine, &site, "schema_version=1")?;
        assert_eq!(variant(validated.effective().stack()), expected, "{name}");
        let raw = validated.effective().machine();
        let canonical: toml::Value = toml::from_str(validated.canonical())?;
        assert!(canonical.get("stack").is_none());
        assert_eq!(toml::Value::try_from(validated.effective())?, canonical);
        let original: toml::Value = toml::from_str(&machine)?;
        match validated.effective().stack() {
            Stack::Astm(stack) => {
                assert_eq!(
                    Some(stack.frame_limit),
                    original
                        .get("framing")
                        .and_then(|f| f.get("limit"))
                        .and_then(toml::Value::as_integer)
                        .and_then(|n| usize::try_from(n).ok())
                );
                assert_eq!(stack.mappings.tests.len(), raw.tests().len());
                assert!(matches!(stack.transport, StreamTransport::Serial { .. }));
                assert_eq!(stack.orders.is_some(), name == "orders.toml");
                if let Some(orders) = &stack.orders {
                    assert!(orders.query.is_some());
                    assert_eq!(orders.layout.test.field, 5);
                }
            }
            Stack::Hl7(stack) => {
                assert_eq!(stack.transport.role, TcpRole::Server);
                assert_eq!(stack.mappings.tests.len(), raw.tests().len());
                assert_eq!(
                    stack.orders.is_some(),
                    matches!(name, "hl7_orders.toml" | "hl7_send.toml")
                );
                if let Some(orders) = &stack.orders {
                    assert_eq!(orders.query.is_some(), name == "hl7_orders.toml");
                    assert_eq!(orders.send.is_some(), name == "hl7_send.toml");
                    if let Some(send) = &orders.send {
                        assert_eq!(send.placer_namespace.as_str(), "LAB");
                    }
                }
            }
            Stack::Law(stack) => {
                assert_eq!(stack.transport.role, TcpRole::Server);
                assert_eq!(stack.policy.query_deadline_ms, 1500);
                assert_eq!(stack.orders.is_some(), name == "law_orders.toml");
                if let Some(orders) = &stack.orders {
                    assert!(!orders.specimen_types.is_empty());
                    assert_eq!(orders.work.application.as_str(), "HOST");
                }
            }
            Stack::Poct(stack) => {
                assert_eq!(stack.policy.depth, 32);
                assert!(!stack.codes.tests.is_empty());
            }
            Stack::VidasLiteral(stack) => {
                assert_eq!(u8::from(stack.policy.sample_limit), 12);
                assert!(!stack.codes.tests.is_empty());
                assert!(stack.orders.is_none());
            }
            Stack::Autolyser(stack) => {
                assert_eq!(stack.policy.poll_ms, 1000);
                assert!(!stack.codes.tests.is_empty());
                assert!(stack.orders.is_none());
            }
            Stack::FileAstm(_) | Stack::FileHl7(_) => panic!("no folder example is shipped"),
        }
    }
    Ok(())
}

#[test]
fn adr0002_stack_is_built_after_overrides_and_keeps_serial_settings() -> TestResult {
    let original = load(ASTM, SITE, "schema_version=1")?;
    let Stack::Astm(stack) = original.effective().stack() else {
        panic!("ASTM stack")
    };
    let StreamTransport::Serial {
        settings,
        operation_timeout_ms,
    } = &stack.transport
    else {
        panic!("serial transport")
    };
    assert_eq!(settings.baud, 9600);
    assert_eq!(*operation_timeout_ms, 10000);
    let overridden = load(
        ASTM,
        SITE,
        r#"schema_version=1
[transport]
kind="tcp_client"
operation_timeout_ms=1234
"#,
    )?;
    let Stack::Astm(stack) = overridden.effective().stack() else {
        panic!("ASTM stack")
    };
    assert!(matches!(
        stack.transport,
        StreamTransport::Tcp {
            role: TcpRole::Client,
            operation_timeout_ms: 1234
        }
    ));
    Ok(())
}

#[test]
fn adr0002_file_pickup_is_a_distinct_stack_for_each_record_syntax() -> TestResult {
    let endpoint = std::env::current_dir()?.to_string_lossy().into_owned();
    let site = SITE.replace(
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
kind="none"
limit=1048576
[dialog]
kind="file_pickup"
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
