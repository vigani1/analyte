//! Commits LAW execution and message dispositions within the interpretation transaction.
use crate::{Receipt, ReconciliationError as Error, key};
use analyte_model::{AwosExecution, Digest, Identifier, OrderState, SourceText};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

/// Located whole-message rejection, retained so a retransmission reproduces AR.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rejection {
    /// One-based OBR occurrence in the original message.
    pub occurrence: u16,
    /// Fixed reason, without patient data.
    pub reason: Conflict,
}
/// Specific correlation and execution conflicts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Conflict {
    /// No single authorized retained work item matches.
    UnknownWork,
    /// Retained work and returned context differ.
    Context,
    /// Execution contradicts retained later evidence.
    Progression,
    /// Same trusted message identity has different bytes.
    Identity,
    /// Non-production context cannot update production work.
    Processing,
}
/// Durable semantic disposition; execution notices never become command outcomes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Disposition {
    /// All outcomes committed atomically.
    Accepted,
    /// Whole message rejected without applying any work/result outcome.
    Rejected(Rejection),
}
/// One exact retained item and its original source context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work {
    /// Retained machine-order identity.
    pub order: i64,
    /// Immutable one-based item index.
    pub item: u16,
    /// Exact assigned placer identity.
    pub placer: Identifier,
    /// Digest of immutable original command used for context comparison.
    pub command: Digest,
    /// Source OBR occurrence for errors.
    pub occurrence: u16,
    /// Execution from an empty group; None denotes a result-bearing group.
    pub execution: Option<AwosExecution>,
    /// Whether result-bearing work reports completion (never means all results are final).
    pub complete: bool,
    /// Original ORC and header timing/estimate evidence; full raw remains in the receipt.
    pub source: SourceText,
}
/// Bounded, host-correlated LAW input; the transaction rechecks retained ownership and progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Custody {
    /// Installed-machine and sender scope, encoded without concatenation ambiguity.
    pub scope: SourceText,
    /// Trusted MSH-10 identity.
    pub message: Identifier,
    /// Exact retained item coverage, including result-bearing groups in mixed messages.
    pub work: Vec<Work>,
    /// Whole-message validation refusal, before execution admission.
    pub rejection: Option<Rejection>,
    /// Host interpretation time, separate from analyzer times.
    pub recorded_at: i64,
}

pub(super) struct Admission {
    pub(super) disposition: Disposition,
    pub(super) replay: bool,
}

pub(super) fn admit(
    tx: &Transaction<'_>,
    receipt: &Receipt,
    custody: &Custody,
    rows: usize,
    reprocessing: bool,
) -> Result<Admission, Error> {
    if custody.work.len() > rows {
        return Err(Error::InvalidInterpretationInput);
    }
    if !reprocessing {
        let exact: Option<String> = tx.query_row("SELECT disposition FROM law_messages WHERE scope=?1 AND message=?2 AND digest=?3 ORDER BY rowid LIMIT 1", params![custody.scope.as_str(),custody.message.as_str(),receipt.metadata.raw_digest.0.as_slice()], |r|r.get(0)).optional()?;
        if let Some(disposition) = exact {
            return Ok(Admission {
                disposition: serde_json::from_str(&disposition)?,
                replay: true,
            });
        }
        let conflict: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM law_messages WHERE scope=?1 AND message=?2)",
            params![custody.scope.as_str(), custody.message.as_str()],
            |r| r.get(0),
        )?;
        if conflict {
            return Ok(Admission {
                disposition: Disposition::Rejected(Rejection {
                    occurrence: 1,
                    reason: Conflict::Identity,
                }),
                replay: false,
            });
        }
    }
    if let Some(rejection) = custody.rejection {
        return Ok(Admission {
            disposition: Disposition::Rejected(rejection),
            replay: false,
        });
    }
    let mut seen = std::collections::HashSet::new();
    for work in &custody.work {
        let rejection = |reason| Admission {
            disposition: Disposition::Rejected(Rejection {
                occurrence: work.occurrence,
                reason,
            }),
            replay: false,
        };
        if !seen.insert((work.order, work.item)) {
            return Ok(rejection(Conflict::Context));
        }
        let order = crate::machine_orders::load(tx, work.order)?;
        let Some(item) = order
            .items
            .iter()
            .find(|i| i.index == work.item && i.placer == work.placer.as_str())
        else {
            return Ok(rejection(Conflict::UnknownWork));
        };
        if order.key.machine != receipt.metadata.machine
            || !item.coverage_known
            || item.tests.is_empty()
            || order
                .original_command()
                .is_none_or(|bytes| crate::digest(bytes) != work.command)
        {
            return Ok(rejection(Conflict::UnknownWork));
        }
        if let Some(incoming) = work.execution {
            if item
                .execution
                .as_ref()
                .is_some_and(|p| p.state.advance(incoming).is_err())
                || matches!(
                    item.state,
                    OrderState::Done | OrderState::Rejected | OrderState::CancelConfirmed
                )
            {
                return Ok(rejection(Conflict::Progression));
            }
            let result_seen: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM interpretations i JOIN receipts r ON r.id=i.receipt,json_each(i.evidence,'$.outcome.Results') result WHERE json_extract(r.metadata,'$.machine')=?1 AND json_extract(result.value,'$.processing')='Production' AND json_extract(result.value,'$.identity.Resolved.observation.order.value')=?2 AND (?3 OR EXISTS(SELECT 1 FROM json_each(result.value,'$.parts') part WHERE json_extract(part.value,'$.source_status') IN ('F','C','X'))))",params![receipt.metadata.machine.as_str(),work.placer.as_str(),incoming != AwosExecution::InProgress],|r|r.get(0))?;
            let work_seen: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM law_work_history WHERE machine_order=?1 AND item_index=?2 AND json_extract(evidence,'$.execution') IS NULL AND (?3 OR json_extract(evidence,'$.complete')=1))",params![work.order,work.item,incoming != AwosExecution::InProgress],|r|r.get(0))?;
            let command_later: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM machine_order_item_history WHERE machine_order=?1 AND item_index=?2 AND (json_extract(evidence,'$.source_code') LIKE '%:CM' OR (?3!='InProgress' AND json_extract(evidence,'$.source_code') LIKE '%:IP')))",params![work.order,work.item,match incoming { AwosExecution::Scheduled => "Scheduled", AwosExecution::InProgress => "InProgress", AwosExecution::AnalyzerCancelled => "AnalyzerCancelled" }],|r|r.get(0))?;
            if result_seen || work_seen || command_later {
                return Ok(rejection(Conflict::Progression));
            }
        } else if item
            .execution
            .as_ref()
            .is_some_and(|p| p.state == AwosExecution::AnalyzerCancelled)
        {
            return Ok(rejection(Conflict::Progression));
        }
    }
    Ok(Admission {
        disposition: Disposition::Accepted,
        replay: false,
    })
}

pub(super) fn write(
    tx: &Transaction<'_>,
    receipt: &Receipt,
    version: i64,
    custody: &Custody,
    admission: &Admission,
) -> Result<(), Error> {
    let receipt_key = key(&receipt.metadata.id)?;
    tx.execute(
        "INSERT INTO law_messages VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            receipt_key,
            version,
            custody.scope.as_str(),
            custody.message.as_str(),
            receipt.metadata.raw_digest.0.as_slice(),
            crate::document(&admission.disposition)?,
            crate::document(custody)?
        ],
    )?;
    if admission.replay || matches!(admission.disposition, Disposition::Rejected(_)) {
        return Ok(());
    }
    for work in &custody.work {
        tx.execute("INSERT INTO law_work_history(receipt,version,machine_order,item_index,evidence) VALUES (?1,?2,?3,?4,?5)",params![receipt_key,version,work.order,work.item,crate::document(work)?])?;
        if let Some(execution) = work.execution {
            let order = crate::machine_orders::load(tx, work.order)?;
            let pending = order
                .items
                .iter()
                .find(|i| i.index == work.item)
                .is_some_and(|i| i.state == OrderState::CancelRequested);
            let state = match execution {
                AwosExecution::Scheduled => "Scheduled",
                AwosExecution::InProgress => "InProgress",
                AwosExecution::AnalyzerCancelled => "AnalyzerCancelled",
            };
            tx.execute("INSERT INTO awos_execution_history(machine_order,item_index,receipt,interpretation,state,recorded_at,cancel_outstanding) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![work.order,work.item,receipt_key,version,state,custody.recorded_at,pending])?;
        }
    }
    Ok(())
}

mod listing;
pub use listing::{AnalyzerCancelledItem, WorkEvidence};
