//! Prepares immutable LAW work references; the store admits execution inside its transaction.
use super::*;
use analyte_hl7::law;
use analyte_store::{LawConflict, LawCustody, LawRejection, LawWork};

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
        .request(deadline()?, move |s| {
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
    let header = law::CustodyHeader::new(&message).ok_or(Error::LawHeader)?;
    let mut custody = new_custody(machine, &header, rejection)?;
    if rejection.is_some() {
        return Ok(Some(custody));
    }
    let production = header.production();
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
    for record in law::custody_records(&message) {
        match record {
            law::CustodyRecord::Work { placer } => {
                occurrence = occurrence.checked_add(1).ok_or(Error::LawObrOccurrence)?;
                let Some(placer) = placer else {
                    work = None;
                    continue;
                };
                work = retained_work(store, machine, placer, occurrence).await?;
                if work.is_none() {
                    custody.rejection = Some(LawRejection {
                        occurrence,
                        reason: LawConflict::UnknownWork,
                    });
                    break;
                }
            }
            law::CustodyRecord::Control { segment, complete } => {
                if let Some(mut item) = work.take() {
                    item.execution = notices
                        .get(&segment.raw().as_ptr().addr())
                        .map(execution_status);
                    item.complete = complete;
                    item.source = analyte_model::SourceText::new(&serde_json::json!({"header_time":std::str::from_utf8(header.timestamp())?,"estimate_zone_source":"MSH-7","notes":controls.get(&segment.raw().as_ptr().addr()).and_then(|parent| notes.get(parent)),"control":std::str::from_utf8(segment.raw())?}).to_string())?;
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
            law::CustodyRecord::Other => {}
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
                .request(deadline()?, move |s| {
                    s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
                })
                .await?;
            let payload = analyte_mapping::read::payload(&machine.snapshot, &receipt.raw);
            match payload
                .ok()
                .and_then(|bytes| law::parse_message(&bytes, settings.options()).ok())
            {
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

async fn retained_work(
    store: &Handle,
    machine: &Machine,
    placer: &[u8],
    occurrence: u16,
) -> Result<Option<LawWork>, Error> {
    let placer = std::str::from_utf8(placer)?.to_owned();
    let target = machine.id.clone();
    let lookup = placer.clone();
    let record = store
        .request(deadline()?, move |s| {
            match s.machine_order_by_placer(&target, &lookup) {
                Err(analyte_store::Error::Conflict) => Ok(None),
                other => other,
            }
        })
        .await?;
    let Some(record) = record else {
        return Ok(None);
    };
    let item = record
        .items
        .iter()
        .find(|i| i.placer == placer)
        .ok_or(Error::RetainedLawItem)?;
    let Some(command) = record.original_command() else {
        return Ok(None);
    };
    Ok(Some(LawWork {
        order: record.id,
        item: item.index,
        placer: Identifier::new(&placer)?,
        command: Digest(analyte_contract::content_digest(command)),
        occurrence,
        execution: None,
        complete: false,
        source: analyte_model::SourceText::new("")?,
    }))
}

fn new_custody(
    machine: &Machine,
    header: &law::CustodyHeader<'_>,
    rejection: Option<crate::orders::law::validation::Mismatch>,
) -> Result<LawCustody, Error> {
    let scope = analyte_model::SourceText::new(&serde_json::to_string(&(
        machine.id.as_str(),
        header.application(),
        header.facility(),
    ))?)?;
    let message_id = Identifier::new(std::str::from_utf8(header.message_id())?)?;
    Ok(LawCustody {
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
    })
}

fn execution_status(status: &law::ExecutionStatus) -> analyte_model::AwosExecution {
    match status {
        law::ExecutionStatus::Scheduled => analyte_model::AwosExecution::Scheduled,
        law::ExecutionStatus::InProgress => analyte_model::AwosExecution::InProgress,
        law::ExecutionStatus::AnalyzerCancelled => analyte_model::AwosExecution::AnalyzerCancelled,
    }
}
