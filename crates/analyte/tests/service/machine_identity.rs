//! Sends synthetic measurements through the selected protocol and checks durable reconciliation.
use super::*;
use analyte_config::Stack;
use std::collections::BTreeMap;
use tokio::io::AsyncWriteExt as _;

fn astm_mappings(
    transport: &analyte_config::RecordTransport<
        Box<analyte_config::AstmStack>,
        Box<analyte_config::FileAstmStack>,
    >,
) -> &analyte_config::Mappings {
    use analyte_config::RecordTransport;
    match transport {
        RecordTransport::Stream(settings) => settings.settings().codes(),
        RecordTransport::Folder(settings) => settings.settings().codes(),
    }
}

pub(super) fn has_measurement_recipe(stack: &Stack) -> bool {
    match stack {
        Stack::Astm(transport) => {
            let mappings = astm_mappings(transport);
            // An evidenced layout with no run selector may still declare a
            // status override. It deliberately leaves measurement identity unresolved.
            mappings.astm_results.is_none() || !mappings.identity.run.is_empty()
        }
        Stack::Hl7(_) => stack.hl7_record_settings().is_some_and(|(_, mappings)| {
            mappings.identity.evidence.is_some() && !mappings.identity.run.is_empty()
        }),
        Stack::Vidas(settings) => settings.settings().codes().vidas_identity.is_some(),
        Stack::Law(_) | Stack::Poct(_) | Stack::Autolyser(_) | Stack::Record(_) => false,
    }
}

// Populate the positions the mapping selects using a synthetic message for
// the protocol. Keep the source measurement fixed while the reading changes.
type RecordFields = BTreeMap<(usize, usize, usize), String>;

fn synthetic_field<'a>(
    records: &mut BTreeMap<&'a str, RecordFields>,
    field: &'a analyte_config::Field,
    value: &str,
) {
    records
        .entry(field.record.as_str())
        .or_default()
        .entry((
            usize::from(field.field),
            usize::from(field.repetition),
            usize::from(field.component),
        ))
        .or_insert_with(|| value.into());
}

fn identity_fields<'a>(
    records: &mut BTreeMap<&'a str, RecordFields>,
    mappings: &'a analyte_config::Mappings,
) {
    use analyte_config::{OrderNamespace, SubId};
    synthetic_field(records, &mappings.sample.field, "00Case");
    synthetic_field(records, &mappings.identity.order_id, "00Case");
    if let OrderNamespace::Field { field } = &mappings.identity.order_namespace {
        synthetic_field(records, field, "synthetic-orders");
    }
    if let SubId::Field(field) = &mappings.identity.sub_id {
        synthetic_field(records, field, "sub1");
    }
    for field in &mappings.identity.run {
        synthetic_field(records, field, "20261127120000");
    }
}

fn synthetic_record(
    record: &str,
    fields: Option<&RecordFields>,
    repeats: &str,
    first_field: usize,
) -> Result<String, Error> {
    let Some(fields) = fields else {
        return Ok(record.into());
    };
    let mut rendered = Vec::<Vec<Vec<&str>>>::new();
    for (&(field, repetition, component), value) in fields {
        let Some(index) = field.checked_sub(first_field) else {
            // The record name in ASTM field 1 is already supplied.
            continue;
        };
        let len = index.checked_add(1).ok_or("field position overflow")?;
        if rendered.len() < len {
            rendered.resize_with(len, Vec::new);
        }
        let field = rendered.get_mut(index).ok_or("field missing")?;
        if field.len() < repetition {
            field.resize_with(repetition, Vec::new);
        }
        let repetition = repetition
            .checked_sub(1)
            .and_then(|at| field.get_mut(at))
            .ok_or("repetitions start at 1")?;
        if repetition.len() < component {
            repetition.resize(component, "");
        }
        *component
            .checked_sub(1)
            .and_then(|at| repetition.get_mut(at))
            .ok_or("components start at 1")? = value;
    }
    let fields = rendered
        .iter()
        .map(|field| {
            field
                .iter()
                .map(|components| components.join("^"))
                .collect::<Vec<_>>()
                .join(repeats)
        })
        .collect::<Vec<_>>()
        .join("|");
    Ok(format!("{record}|{fields}"))
}

fn astm_identity_upload(mappings: &analyte_config::Mappings, value: &str) -> Result<String, Error> {
    use analyte_config::AstmResultLayout;
    let mut records = BTreeMap::new();
    let test_component = mappings
        .test_component
        .ok_or("ASTM test component missing")?;
    let layout = mappings.astm_results.as_ref().map(|profile| profile.layout);
    let (sample_type, test) = match layout {
        Some(
            AstmResultLayout::SlashDilutionAndAlarmCode
            | AstmResultLayout::SlashDilutionAndAlarmCodeWithQcLot,
        ) => ("S1", "123/1/0"),
        Some(AstmResultLayout::CaretDilutionAndAlarm) => ("SAMPLE", "123"),
        Some(AstmResultLayout::MeasurementMarker) | None => ("", "123"),
    };
    records.insert(
        "O",
        BTreeMap::from([((2, 1, 1), "1".into()), ((4, 1, 5), sample_type.into())]),
    );
    let mut result = BTreeMap::from([
        ((2, 1, 1), "1".into()),
        ((3, 1, usize::from(test_component.number())), test.into()),
        ((4, 1, 1), value.into()),
        ((5, 1, 1), "mg/L".into()),
        ((9, 1, 1), "F".into()),
    ]);
    if layout == Some(AstmResultLayout::MeasurementMarker) {
        result.insert((3, 1, 5), "M".into());
    }
    records.insert("R", result);
    records.insert("P", BTreeMap::from([((2, 1, 1), "1".into())]));
    identity_fields(&mut records, mappings);
    let mut upload = String::from("H|\\^&||||||||||P|LIS2-A2|20261127120000\r");
    for record in ["P", "O", "R"] {
        upload.push_str(&synthetic_record(record, records.get(record), "\\", 2)?);
        upload.push('\r');
    }
    upload.push_str("L|1|N\r");
    Ok(upload)
}

fn hl7_identity_upload(
    syntax: &analyte_config::Hl7Syntax,
    mappings: &analyte_config::Mappings,
    id: usize,
    value: &str,
) -> Result<String, Error> {
    use analyte_config::Terminator;
    // HL7 2.5.1 §2.15.9.12: MSH-12 carries the edition's wire spelling.
    let version = analyte_hl7::Version::from(syntax.edition()).as_str();
    let mut records = BTreeMap::from([
        ("PID", BTreeMap::from([((1, 1, 1), "1".into())])),
        (
            "OBR",
            BTreeMap::from([
                ((1, 1, 1), "1".into()),
                ((4, 1, 1), "SYN".into()),
                ((25, 1, 1), "F".into()),
            ]),
        ),
        (
            "OBX",
            BTreeMap::from([
                ((1, 1, 1), "1".into()),
                ((2, 1, 1), "NM".into()),
                ((3, 1, 1), "SYN".into()),
                ((3, 1, 3), "synthetic".into()),
                ((5, 1, 1), value.into()),
                ((6, 1, 1), "mg/L".into()),
                ((11, 1, 1), "F".into()),
            ]),
        ),
    ]);
    identity_fields(&mut records, mappings);
    // A declared MSH-16 category departure (YHLO LIS Protocol Manual V1.1
    // §2.3.1) replaces the acknowledgment fields: MSH-9 is the two-component
    // ORU^R01 and MSH-16 = 0 marks a patient result.
    let (message_type, category) = if syntax.msh16_message_category() {
        ("ORU^R01", "||||0")
    } else {
        ("ORU^R01^ORU_R01", "")
    };
    let mut segments = vec![format!(
        "MSH|^~\\&|SYN|LAB|HOST|LAB|20261127120000||{message_type}|message{id}|P|{version}{category}"
    )];
    for record in ["PID", "OBR", "OBX"] {
        segments.push(synthetic_record(record, records.get(record), "~", 1)?);
    }
    let end = match syntax.terminator() {
        Terminator::Cr => "\r",
        Terminator::CrLf => "\r\n",
        Terminator::Lf => "\n",
    };
    Ok(segments.join(end) + end)
}

pub(super) async fn replay_identity(
    path: &Path,
    incoming: Option<&Path>,
    stack: &Stack,
) -> Result<(), Error> {
    // VIDAS closes each upload session, and its processing mode is unknown.
    // It still reconciles identities, but never enqueues production deliveries.
    let delivery_allowed = !matches!(stack, Stack::Vidas(_));
    let mut peer = if incoming.is_none() && delivery_allowed {
        Some(connect(path, "fixture").await?)
    } else {
        None
    };
    for (index, value) in ["12.30", "12.30", "23.40"].into_iter().enumerate() {
        let count = i64::try_from(index)?
            .checked_add(1)
            .ok_or("upload count overflow")?;
        match stack {
            Stack::Astm(transport) => {
                let upload = astm_identity_upload(astm_mappings(transport), value)?;
                if let Some(incoming) = incoming {
                    std::fs::write(incoming.join(format!("upload-{index}")), upload)?;
                } else {
                    astm(peer.as_mut().ok_or("ASTM peer missing")?, upload.as_bytes()).await?;
                }
            }
            Stack::Hl7(_) => {
                let (syntax, mappings) =
                    stack.hl7_record_settings().ok_or("HL7 mapping missing")?;
                let upload = hl7_identity_upload(syntax, mappings, index, value)?;
                if let Some(incoming) = incoming {
                    std::fs::write(incoming.join(format!("upload-{index}")), upload)?;
                } else {
                    let ack =
                        hl7(peer.as_mut().ok_or("HL7 peer missing")?, upload.as_bytes()).await?;
                    assert!(ack.contains(&format!("MSA|AA|message{index}")), "{ack}");
                }
            }
            Stack::Vidas(settings) => {
                let mut peer = connect(path, "fixture").await?;
                vidas_identity_upload(&mut peer, settings, value).await?;
                recording_eof(&mut peer).await?;
            }
            Stack::Law(_) | Stack::Poct(_) | Stack::Autolyser(_) | Stack::Record(_) => {
                return Err("stack does not have a configurable measurement recipe".into());
            }
        }
        wait_database_count(path, "SELECT count(*) FROM interpretations", [], count).await?;
        let db = database(path)?;
        let held: i64 = db.query_row(
            "SELECT count(*) FROM parked WHERE json_extract(evidence, '$.reason') = 'result reconciliation: Conflict'",
            [],
            |row| row.get(0),
        )?;
        let revisions: i64 =
            db.query_row("SELECT count(*) FROM observation_revisions", [], |row| {
                row.get(0)
            })?;
        let deliveries: i64 = db.query_row(
            "SELECT count(*) FROM outbox WHERE kind = 'result'",
            [],
            |row| row.get(0),
        )?;
        let expected_held = i64::from(index == 2);
        let expected_deliveries = i64::from(delivery_allowed);
        if (revisions, held, deliveries) != (1, expected_held, expected_deliveries) {
            return Err(format!(
                "upload {count}: expected one revision, {expected_held} conflicts and {expected_deliveries} deliveries; found {revisions}, {held} and {deliveries}"
            )
            .into());
        }
    }
    wait_database_count(
        path,
        "SELECT count(*) FROM outbox WHERE kind = 'result' AND state = 'done'",
        [],
        i64::from(delivery_allowed),
    )
    .await?;
    Ok(())
}

async fn vidas_identity_upload(
    peer: &mut tokio::net::TcpStream,
    settings: &analyte_config::VidasStack,
    value: &str,
) -> Result<(), Error> {
    use analyte_framing::vidas::{Checksum, Config, HexOptions, Mode};
    let policy = settings.policy();
    let mode = if policy.ack_point == analyte_config::AckPoint::Etx {
        Mode::Alternate
    } else {
        Mode::Literal
    };
    let framing = Config::new(
        mode,
        policy.chunk,
        settings.settings().frame_limit(),
        &policy.suffix,
        Checksum {
            include_stx: policy.include_stx.into(),
            include_suffix: policy.include_suffix.into(),
            hex: HexOptions {
                uppercase: policy.hex_case.into(),
                accept_either_case: policy.checksum_acceptance.into(),
            },
        },
    )?;
    let body = format!(
        "mtrsl|piLab07|si|ci00Case|rtZQ7|rnSynthetic|tt12:00|td2026/11/27|ql|qn{value} U/mL|y3U/mL|qd2|ncvalid|idSynthPC|"
    );
    let ack = framing.control(6);
    let mut reply = vec![0; ack.len()];
    tokio::time::timeout(HANG_GUARD, async {
        peer.write_all(&framing.control(5)).await?;
        peer.read_exact(&mut reply).await?;
        assert_eq!(reply, ack, "VIDAS establishment ACK");
        peer.write_all(&framing.encode(body.as_bytes())?).await?;
        peer.read_exact(&mut reply).await?;
        assert_eq!(reply, ack, "VIDAS packet ACK");
        if mode == Mode::Literal {
            peer.write_all(&framing.control(3)).await?;
        }
        peer.write_all(&framing.control(4)).await?;
        Ok::<_, Error>(())
    })
    .await?
}
