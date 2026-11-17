//! Prepares immutable LAW work references; the store admits execution inside its transaction.
use super::*;
use analyte_hl7::law;
use analyte_store::{LawConflict, LawCustody, LawRejection, LawWork};

#[expect(
    clippy::too_many_lines,
    reason = "Keep the validated LAW group walk and immutable correlation checks together"
)]
pub(super) async fn plan(
    store: &Handle,
    machine: &Machine,
    results: Option<&law::Results<'_>>,
    rejection: Option<crate::orders::law::validation::Mismatch>,
) -> Result<Option<LawCustody>, Error> {
    let Some(results) = results else {
        return Ok(None);
    };
    let message = results.message();
    let header = law::CustodyHeader::new(message).ok_or(Error::LawHeader)?;
    let scope = analyte_model::SourceText::new(&serde_json::to_string(&(
        machine.id.as_str(),
        header.application(),
        header.facility(),
    ))?)?;
    let message_id = Identifier::new(std::str::from_utf8(header.message_id())?)?;
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
    for record in law::custody_records(message) {
        match record {
            law::CustodyRecord::Work { placer } => {
                occurrence = occurrence.checked_add(1).ok_or(Error::LawObrOccurrence)?;
                let Some(placer) = placer else {
                    work = None;
                    continue;
                };
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
                    .ok_or(Error::RetainedLawItem)?;
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
            law::CustodyRecord::Control { segment, complete } => {
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
    results: Option<&law::Results<'_>>,
) -> Result<Option<crate::orders::law::validation::Mismatch>, Error> {
    let (Some(results), analyte_config::Stack::Law(settings)) =
        (results, machine.snapshot.effective().stack())
    else {
        return Ok(None);
    };
    Ok(
        crate::orders::law::validation::check(
            store,
            machine,
            results.message(),
            settings.options(),
        )
        .await?,
    )
}
