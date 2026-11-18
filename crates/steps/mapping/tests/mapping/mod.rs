//! Tests machine field mapping and exact value retention.
use analyte_config::{Encoding, Escapes, RecordTransport, Stack, SyntaxLimits, Terminator};
#[path = "../settings.rs"]
mod settings;
use analyte_config::{Draft, Validated};
use analyte_mapping::{Input, Output, Problem, astm::map as map_astm, hl7::map as map_hl7};
use analyte_model::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

mod compatibility;
mod continuation;
mod departures;
mod law_text;
mod limits;
mod modern;
#[path = "../mutation/mod.rs"]
mod mutation;
mod patient;
mod values;

// HL7 2.5.1 sections 2.7.4, 2.15.9.18: decode characters once, then delimiter escapes.
fn config(definition: &str) -> Result<Validated, analyte_config::Error> {
    Draft::load(
        ("synthetic", definition),
        ("site", SITE),
        ("local", "schema_version=1"),
    )?
    .validate()
}

fn input() -> Input<'static> {
    Input {
        device: None,
        receipt: ReceiptId::new(),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        revision: Revision(7),
    }
}

fn hl7(body: &str, definition: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(definition)?;
    let raw = format!("MSH|^~\\&|SYN||||202611151200||ORU^R01^ORU_R01|MSG|P|2.5.1\r{body}");
    let options = effective_options(config.effective()).ok_or("missing options")?;
    let message = analyte_hl7::Message::parse(raw.as_bytes(), options)?;
    Ok(map_hl7(&settings::hl7_mapping(&config), &message, input())?)
}

fn astm(body: &str, mode: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let config = config(ASTM_CONFIG)?;
    let raw =
        format!("H|\\^&||||||||||{mode}|LIS2-A2|20261115120000\rP|1\rO|1|000AbC\r{body}L|1|N\r");
    let messages = analyte_astm::parse(raw.as_bytes(), analyte_astm::Limits::default())?;
    let message = messages.first().ok_or("missing parsed message")?;
    Ok(map_astm(
        &settings::astm_mapping(&config),
        message,
        input(),
    )?)
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
        &settings::astm_mapping(&config),
        messages.first().ok_or("missing message")?,
        input(),
    )?)
}

fn parts_config() -> String {
    format!(
        "{ASTM_CONFIG}\n[mappings.component_parts]\ncomponent=4\nseparator='/'\ncount=2\ncode=1\ndilution=2\n"
    )
}

const ASTM_CONFIG: &str = include_str!("../../../../core/config/examples/astm.toml");
const HL7_CONFIG: &str = include_str!("../../../../core/config/examples/hl7.toml");
const SITE: &str = include_str!("../../../../core/config/examples/site.toml");

// Synthetic LAB-29 result shared by profile and edition-boundary regressions.
const LAW_RESULT: &[u8] = b"MSH|^~\\&|A|AF|M|MF|20261115120000+0000||OUL^R22^OUL_R22|result-1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||000tube\rOBR||awos-1||SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|1|NM|SYN^Synthetic^99LAB|1^1^1|1|mg/L^mg/L^UCUM||N^Normal^HL70078|||F|||||operator||instrument^99LAB~module^99LAB|20261115120000||||||||||RSLT\r";

// Supplies explicit synthetic report context for tests of unrelated value/QC rules.
fn qualified_hl7(body: &str, definition: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let mut qualified = String::new();
    for segment in body.split_terminator('\r') {
        if segment.starts_with("OBR|") {
            let mut fields = segment.split('|').map(str::to_owned).collect::<Vec<_>>();
            fields.resize(26, String::new());
            *fields.get_mut(4).ok_or("synthetic OBR-4")? = "SYN^Synthetic^synthetic".into();
            *fields.get_mut(25).ok_or("synthetic OBR-25")? = "F".into();
            qualified.push_str(&fields.join("|"));
        } else {
            qualified.push_str(segment);
        }
        qualified.push('\r');
    }
    hl7(&qualified, definition)
}

fn options_from_values(
    encoding: Encoding,
    terminator: Terminator,
    escapes: Escapes,
    limits: &SyntaxLimits,
) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: false,
        header_zone: analyte_hl7::HeaderZone::Required,
        charset: match encoding {
            Encoding::Ascii => analyte_hl7::Charset::Ascii,
            Encoding::Latin1 => analyte_hl7::Charset::Latin1,
            Encoding::Utf8 => analyte_hl7::Charset::Utf8,
        },
        terminator: match terminator {
            Terminator::Cr => analyte_hl7::Terminator::Cr,
            Terminator::CrLf => analyte_hl7::Terminator::CrLf,
            Terminator::Lf => analyte_hl7::Terminator::Lf,
        },
        escapes: match escapes {
            Escapes::Reject => analyte_hl7::EscapePolicy::Reject,
            Escapes::Preserve => analyte_hl7::EscapePolicy::Preserve,
        },
        limits: analyte_hl7::Limits {
            message_bytes: limits.bytes(),
            segments: limits.records(),
            fields: limits.fields(),
            repetitions: limits.repetitions(),
        },
    }
}

fn options(syntax: &analyte_config::Hl7Syntax) -> analyte_hl7::Options {
    analyte_hl7::Options {
        allow_utf8_in_v231: syntax.allow_utf8_in_v231(),
        ..options_from_values(
            syntax.encoding(),
            syntax.terminator(),
            syntax.escapes(),
            syntax.limits(),
        )
    }
}
fn effective_options(effective: &analyte_config::Effective) -> Option<analyte_hl7::Options> {
    match effective.stack() {
        Stack::Hl7(RecordTransport::Stream(s)) => Some(options(s.settings().syntax())),
        Stack::Hl7(RecordTransport::Folder(s)) => Some(options(s.settings().syntax())),
        Stack::Law(s) => Some(options_from_values(
            Encoding::Utf8,
            Terminator::Cr,
            Escapes::Reject,
            s.settings().limits(),
        )),
        Stack::Astm(_) | Stack::Poct(_) | Stack::Vidas(_) | Stack::Autolyser(_) => None,
    }
}
