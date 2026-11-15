//! Original synthetic messages for manual IHE `PaLM` TF-2b Appendix C validation.
#![forbid(unsafe_code)]
use analyte_hl7::{
    AckCode, AckError, Charset, Message, Options, Response, Responsibility,
    law::{
        self, Address, Awos, CacheAnswer, Capabilities, ContainerDetails, ControlDirection,
        Profile, Query, Results, WorkAction, WorkResponse,
    },
};
use std::{error::Error, io::Write, path::Path};

type ExportResult<T> = Result<T, Box<dyn Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn metadata(id: &[u8]) -> ExportResult<Response<'_>> {
    Ok(Response {
        control_id: law::control_id(id)?,
        timestamp: b"20260101120000+0000",
    })
}
fn analyzer(
    kind: &str,
    id: &str,
    transaction: &str,
    trigger: bool,
    body: &str,
) -> ExportResult<Message> {
    let ack = if trigger { "NE|AL" } else { "|" };
    Ok(law::parse_message(format!("MSH|^~\\&|SYN-ANALYZER|ANALYTE-TEST|ANALYTE|ANALYTE-TEST|20260101120000+0000||{kind}|{id}|P|2.5.1|||{ack}||UNICODE UTF-8|||{transaction}^IHE\r{body}").as_bytes(), options())?)
}
fn capabilities() -> Capabilities {
    Capabilities {
        specimen: true,
        ..Capabilities::default()
    }
}

/// Generates only synthetic messages, validating them before writing exact wire bytes.
pub(crate) fn generate(directory: &Path) -> ExportResult<()> {
    let specimen = analyzer(
        "QBP^Q11^QBP_Q11",
        "SYN-Q1",
        "LAB-27",
        true,
        "QPD|WOS^Work Order Step^IHELAW|SYN-TAG1|SYN-0001^ANALYTE-TEST\rRCP|I||R^Real Time^HL70394\r",
    )?;
    let pending = analyzer(
        "QBP^Q11^QBP_Q11",
        "SYN-Q2",
        "LAB-27",
        true,
        "QPD|WOS_ALL^Work Order Step All^IHELAW|SYN-TAG2\rRCP|I||R^Real Time^HL70394\r",
    )?;
    Query::new(&specimen)?;
    Query::new(&pending)?;
    let found = law::query_response(
        Query::new(&specimen)?,
        CacheAnswer::WorkExists,
        metadata(b"SYN-RSP1")?,
        Responsibility::StoredAndInterpreted,
        options(),
    )?;
    let empty = law::query_response(
        Query::new(&specimen)?,
        CacheAnswer::NoWork,
        metadata(b"SYN-RSP2")?,
        Responsibility::StoredAndInterpreted,
        options(),
    )?;
    let address = Address {
        application: b"ANALYTE",
        facility: b"ANALYTE-TEST",
        peer_application: b"SYN-ANALYZER",
        peer_facility: b"ANALYTE-TEST",
    };
    let container = ContainerDetails {
        parent: Some(b"SYN-PARENT1"),
        carrier: Some(b"SYN-RACK1"),
        carrier_position: Some(b"1"),
        ..ContainerDetails::default()
    };
    let work = |action, id: &'static [u8], container_id: &'static [u8]| -> ExportResult<Message> {
        let awos = Awos::new(
            b"SYN-AWOS1",
            b"SYN-0001&ANALYTE-TEST",
            b"SER^Serum^HL70487",
            b"P",
            container_id,
            b"SYN-TEST^Synthetic test^99SYN",
            action,
        )?
        .with_container_details(&container);
        Ok(law::work_order(
            &[awos],
            None,
            address,
            metadata(id)?,
            capabilities(),
            options(),
        )?)
    };
    let new = work(WorkAction::New, b"SYN-OML1", b"SYN-0001")?;
    let cancel = work(WorkAction::Cancel, b"SYN-OML2", b"SYN-0001")?;
    let parent = work(WorkAction::New, b"SYN-OML4", b"")?;
    let negative = law::negative_work(
        Query::new(&specimen)?,
        address,
        metadata(b"SYN-OML3")?,
        options(),
    )?;
    let accepted = analyzer(
        "ORL^O34^ORL_O42",
        "SYN-ORL1",
        "LAB-28",
        false,
        "MSA|AA|SYN-OML1\rSPM|1|SYN-0001&ANALYTE-TEST||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||SYN-0001\rORC|OK|SYN-AWOS1|||SC\r",
    )?;
    WorkResponse::new(&accepted, &new)?;
    let mut obx = vec![""; 29];
    for (position, value) in [
        (1_usize, "1"),
        (2, "NM"),
        (3, "SYN-TEST^Synthetic test^99SYN"),
        (4, "1"),
        (5, "1.20"),
        (6, "mg/L^mg/L^UCUM"),
        (7, "0-2"),
        (8, "N^Normal^HL70078"),
        (11, "F"),
        (16, "SYN-OPERATOR^Synthetic"),
        (18, "SYN-INSTRUMENT^ANALYTE-TEST~SYN-MODULE^ANALYTE-TEST"),
        (19, "20260101115900"),
        (29, "RSLT"),
    ] {
        *obx.get_mut(
            position
                .checked_sub(1)
                .ok_or("invalid synthetic OBX position")?,
        )
        .ok_or("invalid synthetic OBX position")? = value;
    }
    let result = analyzer(
        "OUL^R22^OUL_R22",
        "SYN-OUL1",
        "LAB-29",
        true,
        &format!(
            "SPM|1|SYN-0001&ANALYTE-TEST||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||SYN-0001\rOBR||SYN-AWOS1||SYN-TEST^Synthetic test^99SYN\rORC|SC||||CM\rOBX|{}\r",
            obx.join("|")
        ),
    )?;
    Results::new(&result)?;
    // Responsibility assertions here describe synthetic scenarios, not real storage.
    let aa = law::result_acknowledgment(
        &result,
        metadata(b"SYN-ACK1")?,
        AckCode::Aa,
        Responsibility::StoredAndInterpreted,
        None,
        options(),
    )?
    .ok_or("AA suppressed")?;
    let ae = law::result_acknowledgment(
        &result,
        metadata(b"SYN-ACK2")?,
        AckCode::Ae,
        Responsibility::Stored,
        Some(AckError {
            segment: b"OBX",
            occurrence: 1,
            field: 3,
            code: b"103",
            application_code: b"SYN_UNMAPPED",
        }),
        options(),
    )?
    .ok_or("AE suppressed")?;
    law::receive_acknowledgment(&aa, &result)?;
    law::receive_acknowledgment(&ae, &result)?;
    std::fs::create_dir_all(directory)?;
    for (name, message, direction) in [
        ("rsp_k11_found.hl7", found.message(), ControlDirection::Send),
        ("rsp_k11_empty.hl7", empty.message(), ControlDirection::Send),
        ("oml_o33_new.hl7", &new, ControlDirection::Send),
        ("oml_o33_cancel.hl7", &cancel, ControlDirection::Send),
        (
            "oml_o33_new_parent_carrier.hl7",
            &parent,
            ControlDirection::Send,
        ),
        ("oml_o33_negative.hl7", &negative, ControlDirection::Send),
        ("ack_r22_aa.hl7", &aa, ControlDirection::Send),
        ("ack_r22_ae.hl7", &ae, ControlDirection::Send),
        ("qbp_q11_specimen.hl7", &specimen, ControlDirection::Receive),
        (
            "qbp_q11_all_pending.hl7",
            &pending,
            ControlDirection::Receive,
        ),
        ("orl_o34_accept.hl7", &accepted, ControlDirection::Receive),
        ("oul_r22.hl7", &result, ControlDirection::Receive),
    ] {
        let reparsed = law::parse_message(message.raw(), options())?;
        law::validate_control(&reparsed, direction)?;
        if name.starts_with("oml_") {
            Profile::for_peer(capabilities()).validate_message(&reparsed, direction)?;
        }
        std::fs::write(directory.join(name), reparsed.raw())?;
        writeln!(
            std::io::stdout(),
            "{name}: {} bytes, locally validated",
            reparsed.raw().len()
        )?;
    }
    Ok(())
}

#[cfg(not(test))]
fn main() -> ExportResult<()> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("usage: law_messages <directory>")?;
    generate(Path::new(&directory))
}
