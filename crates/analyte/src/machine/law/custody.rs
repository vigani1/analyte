//! Prepares immutable LAW work references; the store admits execution inside its transaction.
use crate::{deadline, installation::config::Machine};
use analyte_hl7::law;
use analyte_model::{Digest, Identifier};
use analyte_store::Handle;
use analyte_store::law::{Conflict, Custody, Rejection, Work};

pub(crate) async fn plan(
    store: &Handle,
    machine: &Machine,
    results: Option<&law::Results<'_>>,
    rejection: Option<crate::orders::law::validation::Mismatch>,
) -> Result<Option<Custody>, Error> {
    let Some(results) = results else {
        return Ok(None);
    };
    let message = results.message();
    let header = law::CustodyHeader::new(message).ok_or(Error::Header)?;
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
    for record in law::custody_records(message) {
        match record {
            law::CustodyRecord::Work { placer } => {
                occurrence = occurrence.checked_add(1).ok_or(Error::ObrOccurrence)?;
                let Some(placer) = placer else {
                    work = None;
                    continue;
                };
                work = retained_work(store, machine, placer, occurrence).await?;
                if work.is_none() {
                    custody.rejection = Some(Rejection {
                        occurrence,
                        reason: Conflict::UnknownWork,
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
                    custody.rejection = Some(Rejection {
                        occurrence,
                        reason: Conflict::UnknownWork,
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
        custody.rejection = Some(Rejection {
            occurrence: 1,
            reason: Conflict::Processing,
        });
    }
    Ok(Some(custody))
}

pub(crate) async fn work_conflict(
    store: &Handle,
    machine: &Machine,
    results: Option<&law::Results<'_>>,
) -> Result<Option<crate::orders::law::validation::Mismatch>, Error> {
    let (Some(results), analyte_config::Stack::Law(settings)) =
        (results, machine.snapshot.effective().stack())
    else {
        return Ok(None);
    };
    Ok(crate::orders::law::validation::check(
        store,
        machine,
        results.message(),
        crate::machine::law::settings::options(settings),
    )
    .await?)
}

async fn retained_work(
    store: &Handle,
    machine: &Machine,
    placer: &[u8],
    occurrence: u16,
) -> Result<Option<Work>, Error> {
    let placer = std::str::from_utf8(placer)?.to_owned();
    let record =
        crate::orders::law::validation::retained(store, machine, &placer, deadline()?).await?;
    let Some(record) = record else {
        return Ok(None);
    };
    let item = record
        .items
        .iter()
        .find(|i| i.placer == placer)
        .ok_or(Error::RetainedItem)?;
    let Some(command) = record.original_command() else {
        return Ok(None);
    };
    Ok(Some(Work {
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
) -> Result<Custody, Error> {
    let scope = analyte_model::SourceText::new(&serde_json::to_string(&(
        machine.id.as_str(),
        header.application(),
        header.facility(),
    ))?)?;
    let message_id = Identifier::new(std::str::from_utf8(header.message_id())?)?;
    Ok(Custody {
        scope,
        message: message_id,
        work: Vec::new(),
        rejection: rejection.map(|r| Rejection {
            occurrence: r.occurrence,
            reason: if r.segment == b"OBR" && r.field == 2 {
                Conflict::UnknownWork
            } else {
                Conflict::Context
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

/// LAW context and retained-work failures; none authorize a positive acknowledgement.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// The source message lacks a valid custody header.
    #[error("LAW header")]
    Header,
    /// The source contains more OBR records than custody can represent.
    #[error("LAW OBR occurrence")]
    ObrOccurrence,
    /// Retained work has no matching authorized item.
    #[error("retained LAW item")]
    RetainedItem,
    /// Retained-work comparison could not complete.
    #[error(transparent)]
    Validation(#[from] crate::orders::law::validation::ValidationError),
    /// Storage could not establish durable custody.
    #[error(transparent)]
    Store(#[from] analyte_store::OrderError),
    /// Source text exceeds a model bound.
    #[error(transparent)]
    Text(#[from] analyte_model::TextError),
    /// Original source encoding is invalid.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Protected evidence could not be encoded.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
