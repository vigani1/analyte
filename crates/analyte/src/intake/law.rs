//! Prepares immutable LAW work references; the store admits execution inside its transaction.
use super::*;
use analyte_hl7::{Field, law};
use analyte_store::{LawConflict, LawCustody, LawRejection, LawWork};

#[expect(
    clippy::too_many_lines,
    reason = "Keep the validated LAW group walk and immutable correlation checks together"
)]
pub(super) async fn plan(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    rejection: Option<crate::orders::law::validation::Mismatch>,
) -> Result<Option<LawCustody>, Error> {
    let analyte_config::Stack::Law(settings) = machine.snapshot.effective().stack() else {
        return Ok(None);
    };
    let receipt = store
        .request(deadline(), move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    let Ok(payload) = analyte_mapping::read::payload(&machine.snapshot, &receipt.raw) else {
        // The ordinary mapping failure still needs a durable interpretation and parking.
        return Ok(None);
    };
    let Ok(message) = law::parse_message(&payload, settings.options()) else {
        return Ok(None);
    };
    let Ok(results) = law::receive_results(&message) else {
        return Ok(None);
    };
    let header = message.segments().next().ok_or("LAW header")?;
    let raw =
        |s: analyte_hl7::Segment<'_>, f| s.field(f).map_or(b"".as_slice(), Field::raw).to_vec();
    let scope = analyte_model::SourceText::new(&serde_json::to_string(&(
        machine.id.as_str(),
        raw(header, 3),
        raw(header, 4),
    ))?)?;
    let message_id = Identifier::new(std::str::from_utf8(&raw(header, 10))?)?;
    let mut custody = LawCustody {
        scope,
        message: message_id,
        work: Vec::new(),
        rejection: rejection.map(|r| LawRejection {
            occurrence: r.occurrence,
            reason: if r.segment == b"OBR" && r.field == 2 {
                LawConflict::UnknownWork
            } else {
                LawConflict::Context
            },
        }),
        recorded_at: jiff::Timestamp::now().as_second(),
    };
    if rejection.is_some() {
        return Ok(Some(custody));
    }
    let component = *message
        .delimiters()
        .bytes()
        .get(1)
        .ok_or("LAW component delimiter")?;
    let production = raw(header, 11).split(|b| *b == component).next() == Some(b"P".as_slice());
    let mut notes = std::collections::BTreeMap::<usize, Vec<&str>>::new();
    let mut controls = std::collections::BTreeMap::new();
    let notices = results
        .status_notices()
        .iter()
        .map(|n| (n.control().raw().as_ptr().addr(), n.status()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for attached in results.attached() {
        if let law::Parent::Order(parent) = attached.parent {
            match attached.segment.id() {
                b"NTE" => notes
                    .entry(parent)
                    .or_default()
                    .push(std::str::from_utf8(attached.segment.raw())?),
                b"ORC" => {
                    controls.insert(attached.segment.raw().as_ptr().addr(), parent);
                }
                _ => {}
            }
        }
    }
    let mut occurrence = 0_u16;
    let mut work = None;
    for segment in message.segments() {
        match segment.id() {
            b"OBR" => {
                occurrence = occurrence.checked_add(1).ok_or("LAW OBR occurrence")?;
                let awos = raw(segment, 2);
                if awos == b"\"\"" {
                    work = None;
                    continue;
                }
                let placer = std::str::from_utf8(
                    awos.split(|b| *b == component).next().unwrap_or_default(),
                )?
                .to_owned();
                let target = machine.id.clone();
                let lookup = placer.clone();
                let record = store
                    .request(deadline(), move |s| {
                        match s.machine_order_by_placer(&target, &lookup) {
                            Err(analyte_store::Error::Conflict) => Ok(None),
                            other => other,
                        }
                    })
                    .await?;
                let Some(record) = record else {
                    custody.rejection = Some(LawRejection {
                        occurrence,
                        reason: LawConflict::UnknownWork,
                    });
                    break;
                };
                let item = record
                    .items
                    .iter()
                    .find(|i| i.placer == placer)
                    .ok_or("retained LAW item")?;
                let Some(command) = record.original_command() else {
                    custody.rejection = Some(LawRejection {
                        occurrence,
                        reason: LawConflict::UnknownWork,
                    });
                    break;
                };
                work = Some(LawWork {
                    order: record.id,
                    item: item.index,
                    placer: Identifier::new(&placer)?,
                    command: Digest(analyte_contract::content_digest(command)),
                    occurrence,
                    execution: None,
                    complete: false,
                    source: analyte_model::SourceText::new("")?,
                });
            }
            b"ORC" => {
                if let Some(mut item) = work.take() {
                    item.execution =
                        notices
                            .get(&segment.raw().as_ptr().addr())
                            .map(|status| match status {
                                law::ExecutionStatus::Scheduled => {
                                    analyte_model::AwosExecution::Scheduled
                                }
                                law::ExecutionStatus::InProgress => {
                                    analyte_model::AwosExecution::InProgress
                                }
                                law::ExecutionStatus::AnalyzerCancelled => {
                                    analyte_model::AwosExecution::AnalyzerCancelled
                                }
                            });
                    item.complete = raw(segment, 5) == b"CM";
                    item.source = analyte_model::SourceText::new(&serde_json::json!({"header_time":std::str::from_utf8(&raw(header,7))?,"estimate_zone_source":"MSH-7","notes":controls.get(&segment.raw().as_ptr().addr()).and_then(|parent| notes.get(parent)),"control":std::str::from_utf8(segment.raw())?}).to_string())?;
                    custody.work.push(item);
                } else if results
                    .status_notices()
                    .iter()
                    .any(|n| n.control().raw().as_ptr().addr() == segment.raw().as_ptr().addr())
                {
                    custody.rejection = Some(LawRejection {
                        occurrence,
                        reason: LawConflict::UnknownWork,
                    });
                    break;
                }
            }
            _ => {}
        }
    }
    if !production {
        custody.work.clear();
    }
    if !production && !results.status_notices().is_empty() {
        custody.rejection = Some(LawRejection {
            occurrence: 1,
            reason: LawConflict::Processing,
        });
    }
    Ok(Some(custody))
}

pub(super) async fn work_conflict(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
) -> Result<Option<crate::orders::law::validation::Mismatch>, Error> {
    Ok(
        if let analyte_config::Stack::Law(settings) = machine.snapshot.effective().stack() {
            let receipt = store
                .request(deadline(), move |s| {
                    s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
                })
                .await?;
            let payload = receipt.raw.get(1..receipt.raw.len().saturating_sub(2));
            match payload.and_then(|bytes| law::parse_message(bytes, settings.options()).ok()) {
                Some(message) if law::receive_results(&message).is_ok() => {
                    crate::orders::law::validation::check(
                        store,
                        machine,
                        &message,
                        settings.options(),
                    )
                    .await?
                }
                _ => None,
            }
        } else {
            None
        },
    )
}
