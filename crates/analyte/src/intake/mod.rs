//! Stores receipts, maps results, commits interpretations, revisions and deliveries, and parks failures.
use crate::{Error, deadline, installation::config::Machine, invalid};
use analyte_model::{Digest, Identifier, NamespacedId, ReceiptId, ReceiptMetadata};
use analyte_store::{Handle, Interpretation, Reading, RevisionOutcome};
use std::fmt::Write;
mod law;

/// Binds all evidence from one host connection to its immutable configuration.
pub(crate) struct Connection<'a> {
    machine: &'a Machine,
    connection_id: Identifier,
}

impl<'a> Connection<'a> {
    pub(crate) fn new(machine: &'a Machine) -> Result<Self, Error> {
        Ok(Self {
            machine,
            connection_id: Identifier::new(&uuid::Uuid::now_v7().to_string())?,
        })
    }
}

impl std::ops::Deref for Connection<'_> {
    type Target = Machine;
    fn deref(&self) -> &Machine {
        self.machine
    }
}

fn config_version(machine: &Machine) -> Result<Identifier, Error> {
    let digest = machine.snapshot.digest();
    let mut version = String::with_capacity(64);
    for byte in digest {
        write!(&mut version, "{byte:02x}")?;
    }
    Ok(Identifier::new(&version)?)
}

pub(crate) fn driver(machine: &Machine) -> Result<NamespacedId, Error> {
    Ok(NamespacedId {
        namespace: Identifier::new(match machine.snapshot.effective().stack() {
            analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
            | analyte_config::Stack::Astm(analyte_config::RecordTransport::Folder(_)) => {
                "analyte-astm"
            }
            analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(_))
            | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Folder(_))
            | analyte_config::Stack::Law(_) => "analyte-hl7",
            analyte_config::Stack::Poct(_) => "analyte-poct",
            analyte_config::Stack::VidasLiteral(_) => "analyte-vidas",
            analyte_config::Stack::Autolyser(_) => "analyte-autolyser",
        })?,
        value: Identifier::new(env!("CARGO_PKG_VERSION"))?,
    })
}

pub(crate) async fn receive(
    store: &Handle,
    machine: &Connection<'_>,
    id: ReceiptId,
    raw: Vec<u8>,
) -> Result<jiff::Timestamp, Error> {
    let received_at = jiff::Timestamp::now();
    let config = machine.snapshot.canonical().as_bytes().to_vec();
    let metadata = ReceiptMetadata {
        id,
        raw_digest: Digest(analyte_contract::content_digest(&raw)),
        connection: machine.connection_id.clone(),
        machine: machine.machine.id.clone(),
        driver: driver(machine)?,
        config_version: config_version(machine)?,
        config_digest: Digest(machine.snapshot.digest()),
        received_at,
    };
    store
        .request(deadline(), move |store| {
            store.receive(&metadata, &raw, &config)
        })
        .await?;
    Ok(received_at)
}

/// Durable semantic outcome; conflicts remain distinct from unreadable input.
#[derive(Clone, Copy)]
pub(crate) enum InterpretationOutcome {
    Accepted,
    Failed,
    Conflict,
    WorkConflict(crate::orders::law::validation::Mismatch),
}

pub(crate) async fn interpret(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
) -> Result<bool, Error> {
    Ok(matches!(
        interpret_with_location(store, machine, id).await?.0,
        InterpretationOutcome::Accepted
    ))
}

pub(crate) async fn interpret_with_location(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
) -> Result<
    (
        InterpretationOutcome,
        Option<crate::machine::error_location::Location>,
    ),
    Error,
> {
    let receipt = store
        .request(deadline(), move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    let mapping = crate::machine::interpretation::map_retained(store, machine, &receipt).await;
    let location = machine
        .snapshot
        .effective()
        .hl7_options()
        .and_then(|options| {
            let payload = crate::machine::interpretation::unpack(machine, &receipt.raw).ok()?;
            crate::machine::error_location::mapping_location(&mapping, &payload, options.terminator)
        });
    let accepted = persist_mapping(store, machine, id, mapping, None).await?.0;
    Ok((accepted, location))
}

fn poct_edits(machine: &Machine) -> bool {
    matches!(
        machine.snapshot.effective().stack(),
        analyte_config::Stack::Poct(_)
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep mixed-message acceptance and rejected-batch rollback together"
)]
pub(crate) async fn persist_mapping(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    mapping: Result<analyte_mapping::Output, Error>,
    action: Option<analyte_store::Reprocess>,
) -> Result<(InterpretationOutcome, analyte_store::InterpretationCommit), Error> {
    let poct_edit_guard = poct_edits(machine);
    let atomic = atomic_batch(machine);
    let lifecycle = atomic.then_some(law_lifecycle());
    let work_conflict = law::work_conflict(store, machine, id).await?;
    let mut custody = law::plan(store, machine, id, work_conflict).await?;
    let mapping = if work_conflict.is_some() {
        Err(invalid(
            "LAW returned work differs from retained downloaded command; operator review required",
        ))
    } else {
        mapping
    };
    let MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    } = prepare_reading(map_reading(mapping)?, atomic)?;
    if !accepted && work_conflict.is_none() {
        custody = None;
    }
    let interpretation = Interpretation {
        driver: driver(machine)?,
        config_version: config_version(machine)?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome: reading,
    };
    let destination = machine.snapshot.effective().site().destination.name.clone();
    let conflict = if atomic {
        Some(Interpretation {
            driver: interpretation.driver.clone(),
            config_version: interpretation.config_version.clone(),
            config: interpretation.config.clone(),
            outcome: Reading::Failed(analyte_model::Reason::new(&LawBatchConflict.to_string())?),
        })
    } else {
        None
    };
    let result = store
        .request(deadline(), move |store| {
            let findings = park_findings(id, findings)?;
            let receipt = store
                .receipt(id)?
                .ok_or(analyte_store::Error::Conflict)?
                .metadata;
            let build = |assigned: &Interpretation, outcomes: &[RevisionOutcome]| {
                if atomic
                    && outcomes.iter().any(|outcome| {
                        matches!(
                            outcome,
                            RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                        )
                    })
                {
                    return Err(analyte_store::Error::DeliveryBuild(Box::new(
                        LawBatchConflict,
                    )));
                }
                if !accepted {
                    return Ok(Vec::new());
                }
                build_deliveries(assigned, outcomes, &receipt, &destination, lifecycle)
            };
            let commit = if let Some(custody) = &custody {
                store.interpret_law(
                    id,
                    interpretation.clone(),
                    (&corrections, &findings),
                    lifecycle.ok_or(analyte_store::Error::InvalidInput)?,
                    (custody, action.as_ref()),
                    build,
                )
            } else {
                reconcile(
                    store,
                    id,
                    action.as_ref(),
                    interpretation,
                    (&corrections, &findings, lifecycle, poct_edit_guard),
                    build,
                )
            };
            match commit {
                // The builder refusal rolls back revisions, audit and deliveries together.
                // Keep a failed version (and its audit for operator actions) afterwards.
                Err(analyte_store::Error::DeliveryBuild(error))
                    if error.is::<LawBatchConflict>() =>
                {
                    let rejected_evidence = conflict.ok_or(analyte_store::Error::InvalidInput)?;
                    let rejected = if let Some(mut custody) = custody.clone() {
                        custody.rejection = Some(analyte_store::LawRejection {
                            occurrence: 1,
                            reason: analyte_store::LawConflict::Progression,
                        });
                        store.interpret_law(
                            id,
                            rejected_evidence,
                            (&[], &findings),
                            lifecycle.ok_or(analyte_store::Error::InvalidInput)?,
                            (&custody, action.as_ref()),
                            |_, _| Ok(Vec::new()),
                        )?
                    } else {
                        reconcile(
                            store,
                            id,
                            action.as_ref(),
                            rejected_evidence,
                            (&[], &findings, lifecycle, false),
                            |_, _| Ok(Vec::new()),
                        )?
                    };
                    Ok((rejected, InterpretationOutcome::Conflict))
                }
                result => result.map(|commit| {
                    (
                        commit,
                        if accepted {
                            InterpretationOutcome::Accepted
                        } else {
                            InterpretationOutcome::Failed
                        },
                    )
                }),
            }
        })
        .await?;
    let (result, successful) = result;
    let successful = match result.law {
        Some(analyte_store::LawDisposition::Accepted) => InterpretationOutcome::Accepted,
        Some(analyte_store::LawDisposition::Rejected(rejection)) => {
            InterpretationOutcome::WorkConflict(work_conflict.unwrap_or(
                crate::orders::law::validation::Mismatch {
                    segment: b"OBR",
                    occurrence: rejection.occurrence,
                    field: 2,
                },
            ))
        }
        None => work_conflict.map_or(successful, InterpretationOutcome::WorkConflict),
    };
    mapping_outcome(result, successful)
}

fn prepare_reading(mapped: MappedReading, atomic: bool) -> Result<MappedReading, Error> {
    let MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    } = mapped;
    // A rejected LAW batch must not register otherwise-new history without an
    // outbox delivery: a later accepted retry would be mistaken for a retransmission.
    let (reading, corrections) = if atomic && !accepted && matches!(reading, Reading::Results(_)) {
        (
            Reading::Failed(analyte_model::Reason::new(
                "LAW mapping requires operator review; inspect protected findings and original receipt",
            )?),
            Vec::new(),
        )
    } else {
        (reading, corrections)
    };
    Ok(MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    })
}

fn atomic_batch(machine: &Machine) -> bool {
    match machine.snapshot.effective().stack() {
        analyte_config::Stack::Law(_) => true,
        analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(_))
        | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Stream(_))
        | analyte_config::Stack::Astm(analyte_config::RecordTransport::Folder(_))
        | analyte_config::Stack::Hl7(analyte_config::RecordTransport::Folder(_))
        | analyte_config::Stack::Poct(_)
        | analyte_config::Stack::VidasLiteral(_)
        | analyte_config::Stack::Autolyser(_) => false,
    }
}

fn park_findings(
    id: ReceiptId,
    findings: Vec<String>,
) -> Result<Vec<analyte_store::Parked>, analyte_store::Error> {
    findings
        .into_iter()
        .map(|finding| {
            Ok(analyte_store::Parked {
                kind: Identifier::new("mapping_finding")
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                reason: analyte_model::Reason::new(&finding)
                    .map_err(|_| analyte_store::Error::InvalidInput)?,
                targets: vec![
                    Identifier::new(&uuid::Uuid::from(id).to_string())
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                ],
            })
        })
        .collect::<Result<Vec<_>, analyte_store::Error>>()
}

// One audit dispatch for normal and rejected-batch commits; recovery never bypasses it.
fn reconcile(
    store: &mut analyte_store::Store,
    id: ReceiptId,
    action: Option<&analyte_store::Reprocess>,
    interpretation: Interpretation,
    mapping: (
        &[bool],
        &[analyte_store::Parked],
        Option<analyte_store::StatusLifecycle<'_>>,
        bool,
    ),
    build: impl FnOnce(
        &Interpretation,
        &[RevisionOutcome],
    ) -> Result<Vec<analyte_store::Delivery>, analyte_store::Error>,
) -> Result<analyte_store::InterpretationCommit, analyte_store::Error> {
    let (corrections, findings, lifecycle, edit_guard) = mapping;
    if edit_guard {
        return store.interpret_with_edit_guard(id, interpretation,
            (corrections, findings, "edited POCT observation with no detectable common-content change; source context may have changed"), action, build);
    }
    match (action, lifecycle) {
        (Some(action), Some(policy)) => store.reprocess_with_lifecycle(
            action,
            interpretation,
            (corrections, findings),
            policy,
            build,
        ),
        (None, Some(policy)) => store.interpret_with_lifecycle(
            id,
            interpretation,
            (corrections, findings),
            policy,
            build,
        ),
        (Some(action), None) => {
            store.reprocess_reconciled(action, interpretation, corrections, findings, build)
        }
        (None, None) => {
            store.interpret_reconciled(id, interpretation, corrections, findings, build)
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "LAW result batch conflicts with retained observation history; no result revisions committed"
)]
struct LawBatchConflict;

// The store assigns revisions before this callback freezes bytes. Only new and
// corrected production results can cross this boundary; replay never enqueues.
fn build_deliveries(
    assigned: &Interpretation,
    outcomes: &[RevisionOutcome],
    receipt: &ReceiptMetadata,
    destination: &Identifier,
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
) -> Result<Vec<analyte_store::Delivery>, analyte_store::Error> {
    let mut deliveries = Vec::new();
    if let Reading::Results(results) = &assigned.outcome {
        for (result, outcome) in results.iter().zip(outcomes) {
            let fresh = match outcome {
                RevisionOutcome::New(_) | RevisionOutcome::Correction(_) => true,
                RevisionOutcome::Retransmission(_)
                | RevisionOutcome::Conflict
                | RevisionOutcome::Unresolved => false,
            };
            if fresh
                && lifecycle.is_none_or(|policy| policy.may_deliver(result))
                && result.processing.may_deliver_to_production()
                && !result.requires_parking()
            {
                deliveries.push(
                    analyte_sink::build_delivery(
                        result,
                        receipt,
                        destination.clone(),
                        jiff::Timestamp::now(),
                    )
                    .map_err(|error| analyte_store::Error::DeliveryBuild(Box::new(error)))?,
                );
            }
        }
    }
    Ok(deliveries)
}

pub(crate) async fn park_bytes(
    store: &Handle,
    machine: &Connection<'_>,
    bytes: Vec<u8>,
    reason: &str,
) -> Result<(), Error> {
    let id = ReceiptId::new();
    receive(store, machine, id, bytes).await?;
    let evidence = Interpretation {
        driver: driver(machine)?,
        config_version: config_version(machine)?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome: Reading::Failed(analyte_model::Reason::new(reason)?),
    };
    store
        .request(deadline(), move |s| {
            s.interpret_reconciled(id, evidence, &[], &[], |_, _| Ok(Vec::new()))
        })
        .await?;
    Ok(())
}

/// Saves vendor model evidence through the same revision and delivery transaction.
/// Cancellation may leave a committed interpretation; never infer failure from timeout.
pub(crate) async fn vendor_reading(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    output: analyte_mapping::Output,
) -> Result<(), Error> {
    persist_mapping(store, machine, id, Ok(output), None).await?;
    Ok(())
}

pub(crate) async fn failed_reading(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    reason: &str,
) -> Result<(), Error> {
    persist_mapping(store, machine, id, Err(invalid(reason)), None).await?;
    Ok(())
}

fn law_lifecycle() -> analyte_store::StatusLifecycle<'static> {
    analyte_store::StatusLifecycle {
        transitions: analyte_hl7::law::STATUS_TRANSITIONS,
        withheld: analyte_hl7::law::WITHHELD_STATUSES,
        single_run: analyte_hl7::law::SINGLE_RUN_STATUSES,
        run: |identity| {
            analyte_hl7::law::observation_run(identity.sub_id.as_str()).map(str::to_owned)
        },
    }
}

struct MappedReading {
    accepted: bool,
    reading: Reading,
    corrections: Vec<bool>,
    findings: Vec<String>,
}

fn map_reading(mapping: Result<analyte_mapping::Output, Error>) -> Result<MappedReading, Error> {
    let mut accepted = false;
    let (reading, corrections, findings) = match mapping {
        Ok(output) => {
            accepted = !output.requires_parking();
            let mut findings = output
                .findings
                .iter()
                .map(|f| format!("{:?}: {:?}", f.position, f.problem))
                .collect::<Vec<_>>();
            for result in &output.results {
                if let analyte_model::IdentityStatus::Unresolved(reason) = &result.content.identity
                {
                    findings.push(reason.as_str().to_owned());
                }
            }
            let corrections = output
                .results
                .iter()
                .map(|r| r.corrections.as_slice().iter().any(|c| *c))
                .collect::<Vec<_>>();
            (
                Reading::Results(output.results.into_iter().map(|r| r.content).collect()),
                corrections,
                findings,
            )
        }
        // Parser diagnostics can contain source tokens; keep them only in protected evidence.
        Err(error) => (
            Reading::Failed(analyte_model::Reason::new(&error.to_string())?),
            Vec::new(),
            Vec::new(),
        ),
    };
    Ok(MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    })
}

fn mapping_outcome(
    result: analyte_store::InterpretationCommit,
    successful: InterpretationOutcome,
) -> Result<(InterpretationOutcome, analyte_store::InterpretationCommit), Error> {
    let accepted = matches!(successful, InterpretationOutcome::Accepted)
        && !result
            .outcomes
            .iter()
            .any(|o| matches!(o, RevisionOutcome::Conflict | RevisionOutcome::Unresolved));
    Ok((
        if accepted {
            InterpretationOutcome::Accepted
        } else {
            match successful {
                InterpretationOutcome::Conflict => InterpretationOutcome::Conflict,
                InterpretationOutcome::WorkConflict(location) => {
                    InterpretationOutcome::WorkConflict(location)
                }
                InterpretationOutcome::Accepted | InterpretationOutcome::Failed => {
                    InterpretationOutcome::Failed
                }
            }
        },
        result,
    ))
}

#[cfg(test)]
mod tests;
