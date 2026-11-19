//! Checks configuration rejection and immutable validated settings using synthetic inputs.
use super::*;
const SITE: &str = include_str!("../examples/site.toml");
const ASTM: &str = include_str!("../examples/astm.toml");
const HL7: &str = include_str!("../examples/hl7.toml");

// ADR 0002
#[test]
fn every_example_keeps_legacy_canonical_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
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

    for (name, base, _) in cases {
        let text = std::fs::read_to_string(directory.join(name))?;
        let (machine, site) = if name == "site.toml" {
            (base.to_owned(), text)
        } else {
            (format!("{base}\n{text}"), SITE.to_owned())
        };
        let parsed_machine: MachineDocument = load("machine", &machine)?;
        let (parsed_site, _) = retention::load_site("site", &site, &LabApps::none(LAB_APPS_FILE))?;
        let legacy = toml::Value::try_from(Canonical {
            schema_version: 1,
            machine: &parsed_machine,
            site: &parsed_site,
        })?;
        let validated = Draft::load(
            ("machine", &machine),
            ("site", &site),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        assert_eq!(toml::to_string(&legacy)?, validated.canonical(), "{name}");
    }
    Ok(())
}

// ADR 0002
#[test]
fn stack_conversion_reports_missing_setting() -> Result<(), Error> {
    let mut machine: MachineDocument = load("machine", ASTM)?;
    machine.framing.checksum_hex = None;
    assert!(
        matches!(Stack::parse(&machine, "machine"), Err(Error::Invalid { key, .. }) if key == "framing.checksum_hex")
    );
    let mut machine: MachineDocument = load("machine", HL7)?;
    machine.syntax.edition = None;
    assert!(
        matches!(Stack::parse(&machine, "machine"), Err(Error::Invalid { key, .. }) if key == "syntax.edition")
    );
    Ok(())
}

// HL7 2.5.1 §2.15.9.12
#[test]
fn unsupported_edition_lists_qualified_editions() {
    let error = Error::UnsupportedHl7Edition {
        path: "machine.toml".into(),
        edition: "2.7",
    };
    assert!(
        error
            .to_string()
            .ends_with("supported: 2.3, 2.3.1, 2.4, 2.5, 2.5.1, 2.6, 2.8.2, 2.9.1")
    );
}

// ADR 0002
#[test]
fn machine_schema1_is_refused_as_unsupported() -> Result<(), Box<dyn std::error::Error>> {
    for current in [ASTM, HL7] {
        let old = current.replace("schema_version = 2", "schema_version = 1");
        let refusal = Draft::load(
            ("machine", &old),
            ("site", SITE),
            ("overrides", "schema_version=1"),
        )
        .expect_err("schema 1 must be refused");
        assert!(
            matches!(&refusal, Error::Invalid { key, reason, .. }
                if key == "schema_version" && reason.starts_with("unsupported schema")),
            "unexpected refusal: {refusal:?}"
        );
    }
    Ok(())
}

const LAB_APPS: &str = include_str!("../examples/lab-apps.toml");

fn digest_hex(validated: &Validated) -> String {
    validated
        .digest()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validated(machine: &str, site: &str, lab_apps: &LabApps) -> Result<Validated, Error> {
    Draft::load_with(
        ("machine", machine),
        ("site", site),
        ("overrides", "schema_version = 1"),
        lab_apps,
    )?
    .validate()
}

/// The example site file naming its lab app instead of writing it.
fn named_site() -> String {
    let at = SITE.find("[destination]").unwrap_or(SITE.len());
    format!("{}lab_app = \"demo\"\n", SITE.get(..at).unwrap_or_default())
}

// Owner request 2026-11-19 (lab app set once), item 6; ADR 0002: an old site file
// that writes its own [destination] loads exactly as before. These digests were
// taken from the examples before lab-apps.toml existed; they must never change.
#[test]
fn old_site_file_keeps_its_digest() -> Result<(), Error> {
    let none = LabApps::none(LAB_APPS_FILE);
    let lab_apps = LabApps::load(LAB_APPS_FILE, LAB_APPS)?;
    for (machine, digest) in [
        (
            HL7,
            "81c329b6bbb8b10728b175b250892c65107bf1e6987a663e06ab636653801e94",
        ),
        (
            ASTM,
            "569cecf20e8ec67f51c580e34c0e3b565bbb74b5b164a496f1a276c03055bc8e",
        ),
    ] {
        let alone = validated(machine, SITE, &none)?;
        assert_eq!(digest_hex(&alone), digest);
        assert_eq!(alone.effective().lab_app_source(), LabAppSource::Site);
        // A lab-apps.toml with the same settings beside it changes nothing.
        assert_eq!(digest_hex(&validated(machine, SITE, &lab_apps)?), digest);
    }
    Ok(())
}

// Owner request 2026-11-19 (lab app set once), items 2 and 6: lab_app names a lab
// app in lab-apps.toml; the effective snapshot holds the same destination values,
// so its bytes and digest equal the old embedded form's.
#[test]
fn named_lab_app_gives_the_same_snapshot() -> Result<(), Error> {
    let lab_apps = LabApps::load(LAB_APPS_FILE, LAB_APPS)?;
    let embedded = validated(HL7, SITE, &lab_apps)?;
    let named = validated(HL7, &named_site(), &lab_apps)?;
    assert_eq!(named.canonical(), embedded.canonical());
    assert_eq!(named.digest(), embedded.digest());
    assert!(!named.canonical().contains("lab_app"));
    assert_eq!(named.effective().lab_app_source(), LabAppSource::LabApps);
    assert_eq!(named.effective().site().destination.name.as_str(), "demo");
    Ok(())
}

fn refusal(machine: &str, site: &str, lab_apps: &LabApps) -> Result<LabAppProblem, String> {
    match validated(machine, site, lab_apps) {
        Err(Error::LabApp { path, problem }) if path == "site" => Ok(problem),
        Err(other) => Err(format!("unexpected refusal: {other}")),
        Ok(_) => Err("accepted".into()),
    }
}

// Owner request 2026-11-19 (lab app set once), item 2: a site file that names no
// lab app uses the only one; with several it is refused and asked to choose; an
// unknown name, no lab app at all, and both forms at once are refused.
#[test]
fn lab_app_choice_is_never_guessed() -> Result<(), Box<dyn std::error::Error>> {
    let one = LabApps::load(LAB_APPS_FILE, LAB_APPS)?;
    let at = SITE.find("[destination]").ok_or("example site")?;
    let unnamed = SITE.get(..at).ok_or("example site")?;
    let only = validated(HL7, unnamed, &one)?;
    assert_eq!(only.digest(), validated(HL7, SITE, &one)?.digest());
    let two = one.with(toml::from_str(concat!(
        "name = 'second'\nbase_url = 'https://second.example.invalid'\n",
        "api_key_name = 'second-api'\nsigning_key_name = 'second-signing'\n"
    ))?)?;
    assert_eq!(
        refusal(HL7, unnamed, &two)?,
        LabAppProblem::Choose {
            file: LAB_APPS_FILE.into()
        }
    );
    let chosen = format!("{unnamed}lab_app = \"second\"\n");
    let second = validated(HL7, &chosen, &two)?;
    assert_eq!(
        second.effective().site().destination.name.as_str(),
        "second"
    );
    let unknown = format!("{unnamed}lab_app = \"third\"\n");
    assert_eq!(
        refusal(HL7, &unknown, &two)?,
        LabAppProblem::Unknown {
            name: "third".into(),
            file: LAB_APPS_FILE.into()
        }
    );
    let none = LabApps::none(LAB_APPS_FILE);
    assert_eq!(refusal(HL7, unnamed, &none)?, LabAppProblem::Missing);
    assert!(matches!(
        refusal(HL7, &named_site(), &none)?,
        LabAppProblem::Unknown { .. }
    ));
    let both = format!("lab_app = \"demo\"\n{SITE}");
    assert_eq!(refusal(HL7, &both, &one)?, LabAppProblem::Both);
    let message = Error::LabApp {
        path: "site".into(),
        problem: LabAppProblem::Choose {
            file: LAB_APPS_FILE.into(),
        },
    }
    .to_string();
    assert_eq!(
        message,
        "site: lab-apps.toml lists several lab apps; choose one in this site file with lab_app = \"<name>\""
    );
    Ok(())
}

// Owner request 2026-11-19 (lab app set once), item 1: lab-apps.toml is checked
// like a site file's [destination], each name once, unknown keys refused, and
// the text the page writes loads back to the same lab apps.
#[test]
fn lab_apps_file_is_checked_like_a_destination() -> Result<(), Box<dyn std::error::Error>> {
    let lab_apps = LabApps::load(LAB_APPS_FILE, LAB_APPS)?;
    assert_eq!(LabApps::load(LAB_APPS_FILE, &lab_apps.text()?)?, lab_apps);
    let twice = format!(
        "{LAB_APPS}{}",
        LAB_APPS
            .get(LAB_APPS.find("[[lab_apps]]").ok_or("table")?..)
            .ok_or("table")?
    );
    assert!(matches!(
        LabApps::load(LAB_APPS_FILE, &twice),
        Err(Error::LabApp { problem: LabAppProblem::Duplicate(name), .. }) if name == "demo"
    ));
    for (bad, key) in [
        (
            "request_timeout_seconds = 121\n",
            "lab_apps.request_timeout_seconds",
        ),
        ("freshness_seconds = 0\n", "lab_apps.freshness_seconds"),
    ] {
        let error = LabApps::load(LAB_APPS_FILE, &format!("{LAB_APPS}{bad}"))
            .err()
            .ok_or("accepted")?;
        assert!(
            matches!(&error, Error::Invalid { key: found, .. } if found == key),
            "{error}"
        );
    }
    for bad in [
        format!("{LAB_APPS}endpoint = '127.0.0.1:1'\n"),
        LAB_APPS.replace("https://", "http://"),
        LAB_APPS.replace("schema_version = 1", "schema_version = 2"),
    ] {
        assert!(LabApps::load(LAB_APPS_FILE, &bad).is_err(), "{bad}");
    }
    assert!(lab_apps.without("demo").listed().is_empty());
    Ok(())
}
