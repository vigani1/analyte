//! Supplies synthetic mapping test fixtures and scenarios.
use analyte_config::{Draft, Validated};
use analyte_mapping::{Input, Output, Problem, map_astm, map_hl7};
use analyte_model::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[path = "../mutation/mod.rs"]
mod mutation;

// HL7 2.5.1 sections 2.7.4, 2.15.9.18: decode characters once, then delimiter escapes.
fn config(definition: &str) -> Result<Validated, analyte_config::Error> {
    Draft::load(
        ("synthetic", definition),
        ("site", SITE),
        ("local", "schema_version=1"),
    )?
    .validate()
}

fn input() -> Input {
    Input {
        receipt: ReceiptId::new(),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        revision: Revision(7),
    }
}

fn hl7(body: &str, definition: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(definition)?;
    let raw = format!("MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\r{body}");
    let options = config.effective().hl7_options().ok_or("missing options")?;
    let message = analyte_hl7::Message::parse(raw.as_bytes(), options)?;
    Ok(map_hl7(&message, &config, input())?)
}

fn astm(body: &str, mode: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(ASTM_CONFIG)?;
    let raw =
        format!("H|\\^&||||||||||{mode}|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\r{body}L|1|N\r");
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    let message = messages.first().ok_or("missing parsed message")?;
    Ok(map_astm(message, &config, input())?)
}

fn first(output: &Output) -> Result<&ResultPart, Box<dyn std::error::Error>> {
    Ok(output
        .results
        .first()
        .and_then(|r| r.content.parts.as_slice().first())
        .ok_or("missing result")?)
}

fn with_parts(component: &str, definition: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(definition)?;
    let raw = format!(
        "H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\rR|1|^^^{component}|0.10|||||F\rL|1|N\r"
    );
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    Ok(map_astm(
        messages.first().ok_or("missing message")?,
        &config,
        input(),
    )?)
}

fn parts_config() -> String {
    format!(
        "{ASTM_CONFIG}\n[mappings.component_parts]\ncomponent=4\nseparator='/'\ncount=2\ncode=1\ndilution=2\n"
    )
}

mod scenarios_1;

mod scenarios_2;

const ASTM_CONFIG: &str = include_str!("../../../../core/config/examples/astm.toml");
const HL7_CONFIG: &str = include_str!("../../../../core/config/examples/hl7.toml");
const SITE: &str = include_str!("../../../../core/config/examples/site.toml");
