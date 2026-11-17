//! Stores receipts, maps results, commits interpretations, revisions and deliveries, and parks failures.
use crate::{deadline, installation::config::Machine};
mod bytes;
mod error;
use analyte_model::{Digest, Identifier, NamespacedId, ReceiptId, ReceiptMetadata};
use analyte_store::{Handle, Interpretation, Reading, RevisionOutcome};
pub(crate) use bytes::ByteReason;
pub use error::IntakeError as Error;
use std::fmt::Write;
mod law;
mod parking;
mod reading;
pub(crate) use parking::{Mapping, ParkReason};
use reading::{map_reading, mapping_outcome};

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
        namespace: Identifier::new(machine.snapshot.effective().stack().driver_namespace())?,
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
        .request(deadline()?, move |store| {
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
        Option<crate::machine::hl7::error_location::Location>,
    ),
    Error,
> {
    let receipt = store
        .request(deadline()?, move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    let mapping = crate::machine::interpretation::map_retained(store, machine, &receipt).await;
    let location = machine
        .snapshot
        .effective()
        .hl7_options()
        .and_then(|options| {
            let payload = analyte_mapping::read::payload(&machine.snapshot, &receipt.raw).ok()?;
            crate::machine::hl7::error_location::mapping_location(
                &mapping,
                &payload,
                options.terminator,
            )
        });
    // Preserve the vendor diagnostics historically retained for unreadable input.
    let mapping = match mapping {
        Ok(output) => Mapping::Mapped(output),
        Err(error) => {
            let reason = match machine.snapshot.effective().stack() {
                analyte_config::Stack::Vidas(_) => format!("VIDAS interpretation: {error}"),
                analyte_config::Stack::Autolyser(_) => format!("Autolyser interpretation: {error}"),
                analyte_config::Stack::Poct(_) => format!("POCT interpretation: {error}"),
                analyte_config::Stack::Astm(_)
                | analyte_config::Stack::Hl7(_)
                | analyte_config::Stack::Law(_) => error.to_string(),
            };
            Mapping::Parked(ParkReason::Diagnostic(reason))
        }
    };
    let accepted = persist_mapping(store, machine, id, mapping, None).await?.0;
    Ok((accepted, location))
}

fn poct_edits(machine: &Machine) -> bool {
    machine.snapshot.effective().stack().independent_readings()
}

fn reading_evidence(
    machine: &Machine,
    outcome: Reading,
    departures: analyte_model::DepartureEvidence,
) -> Result<Interpretation, Error> {
    Ok(Interpretation {
        driver: driver(machine)?,
        config_version: config_version(machine)?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome,
        departures: Some(departures),
    })
}

pub(crate) async fn persist_mapping(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    mapping: Mapping,
    action: Option<analyte_store::Reprocess>,
) -> Result<(InterpretationOutcome, analyte_store::InterpretationCommit), Error> {
    let (mapping, departures) = super::machine::interpretation::departure_evidence::for_receipt(
        store, machine, id, mapping,
    )
    .await?;
    let poct_edit_guard = poct_edits(machine);
    let atomic = atomic_batch(machine);
    let lifecycle = atomic.then_some(law_lifecycle());
    let work_conflict = law::work_conflict(store, machine, id).await?;
    let mut custody = law::plan(store, machine, id, work_conflict).await?;
    let mapping = if work_conflict.is_some() {
        Mapping::Parked(ParkReason::WorkConflict)
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
    let interpretation = reading_evidence(machine, reading, departures)?;
    let destination = machine.snapshot.effective().site().destination.name.clone();
    let conflict = if atomic {
        Some(Interpretation {
            departures: interpretation.departures.clone(),
            driver: interpretation.driver.clone(),
            config_version: interpretation.config_version.clone(),
            config: interpretation.config.clone(),
            outcome: Reading::Failed(analyte_model::Reason::new(&LawBatchConflict.to_string())?),
        })
    } else {
        None
    };
    let prepared = PreparedMapping {
        id,
        findings,
        corrections,
        interpretation,
        destination,
        conflict,
        custody,
        action,
        lifecycle,
        atomic,
        accepted,
        poct_edit_guard,
    };
    let result = store
        .request(deadline()?, move |store| prepared.commit(store))
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

struct PreparedMapping {
    id: ReceiptId,
    findings: Vec<String>,
    corrections: Vec<bool>,
    interpretation: Interpretation,
    destination: Identifier,
    conflict: Option<Interpretation>,
    custody: Option<analyte_store::LawCustody>,
    action: Option<analyte_store::Reprocess>,
    lifecycle: Option<analyte_store::StatusLifecycle<'static>>,
    atomic: bool,
    accepted: bool,
    poct_edit_guard: bool,
}

impl PreparedMapping {
    fn commit(
        self,
        store: &mut analyte_store::Store,
    ) -> Result<(analyte_store::InterpretationCommit, InterpretationOutcome), analyte_store::Error>
    {
        let Self {
            id,
            findings,
            corrections,
            interpretation,
            destination,
            conflict,
            custody,
            action,
            lifecycle,
            atomic,
            accepted,
            poct_edit_guard,
        } = self;
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
            store.reconcile(
                action.as_ref().map_or(
                    analyte_store::ReconcileAction::Interpret(id),
                    analyte_store::ReconcileAction::Reprocess,
                ),
                interpretation.clone(),
                analyte_store::ReconcileInput {
                    corrections: &corrections,
                    findings: &findings,
                    mode: analyte_store::ReconcileMode::Law {
                        lifecycle: lifecycle.ok_or(analyte_store::Error::InvalidHostInput)?,
                        custody,
                    },
                },
                build,
            )
        } else {
            store.reconcile(
                reconciliation_action(id, action.as_ref()),
                interpretation,
                analyte_store::ReconcileInput {
                    corrections: &corrections,
                    findings: &findings,
                    mode: reconciliation_mode(lifecycle, poct_edit_guard),
                },
                build,
            )
        };
        match commit {
            // The builder refusal rolls back revisions, audit and deliveries together.
            // Keep a failed version (and its audit for operator actions) afterwards.
            Err(analyte_store::Error::DeliveryBuild(error)) if error.is::<LawBatchConflict>() => {
                let rejected = reject_batch(
                    store,
                    id,
                    conflict,
                    custody.as_ref(),
                    action.as_ref(),
                    &findings,
                    lifecycle,
                )?;
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
    }
}

fn reconciliation_action(
    id: ReceiptId,
    action: Option<&analyte_store::Reprocess>,
) -> analyte_store::ReconcileAction<'_> {
    action.map_or(
        analyte_store::ReconcileAction::Interpret(id),
        analyte_store::ReconcileAction::Reprocess,
    )
}

fn reject_batch(
    store: &mut analyte_store::Store,
    id: ReceiptId,
    conflict: Option<Interpretation>,
    custody: Option<&analyte_store::LawCustody>,
    action: Option<&analyte_store::Reprocess>,
    findings: &[analyte_store::Parked],
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
) -> Result<analyte_store::InterpretationCommit, analyte_store::Error> {
    let rejected_evidence = conflict.ok_or(analyte_store::Error::InvalidHostInput)?;
    let rejected = if let Some(mut custody) = custody.cloned() {
        custody.rejection = Some(analyte_store::LawRejection {
            occurrence: 1,
            reason: analyte_store::LawConflict::Progression,
        });
        store.reconcile(
            action.map_or(
                analyte_store::ReconcileAction::Interpret(id),
                analyte_store::ReconcileAction::Reprocess,
            ),
            rejected_evidence,
            analyte_store::ReconcileInput {
                corrections: &[],
                findings,
                mode: analyte_store::ReconcileMode::Law {
                    lifecycle: lifecycle.ok_or(analyte_store::Error::InvalidHostInput)?,
                    custody: &custody,
                },
            },
            |_, _| Ok(Vec::new()),
        )?
    } else {
        store.reconcile(
            reconciliation_action(id, action),
            rejected_evidence,
            analyte_store::ReconcileInput {
                corrections: &[],
                findings,
                mode: reconciliation_mode(lifecycle, false),
            },
            |_, _| Ok(Vec::new()),
        )?
    };

    Ok(rejected)
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
    machine.snapshot.effective().stack().atomic_batches()
}

fn park_findings(
    id: ReceiptId,
    findings: Vec<String>,
) -> Result<Vec<analyte_store::Parked>, analyte_store::Error> {
    findings
        .into_iter()
        .map(|finding| {
            Ok(analyte_store::Parked {
                kind: Identifier::new("mapping_finding").map_err(analyte_store::Error::Text)?,
                reason: analyte_model::Reason::new(&finding).map_err(analyte_store::Error::Text)?,
                targets: vec![
                    Identifier::new(&uuid::Uuid::from(id).to_string())
                        .map_err(analyte_store::Error::Text)?,
                ],
            })
        })
        .collect::<Result<Vec<_>, analyte_store::Error>>()
}

fn reconciliation_mode(
    lifecycle: Option<analyte_store::StatusLifecycle<'_>>,
    edit_guard: bool,
) -> analyte_store::ReconcileMode<'_> {
    if edit_guard {
        analyte_store::ReconcileMode::EditGuard(
            "edited POCT observation with no detectable common-content change; source context may have changed",
        )
    } else {
        lifecycle.map_or(
            analyte_store::ReconcileMode::Standard,
            analyte_store::ReconcileMode::Lifecycle,
        )
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
                    analyte_sink::encode_delivery(
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
    reason: ByteReason<'_>,
) -> Result<(), Error> {
    let id = ReceiptId::new();
    let departures =
        super::machine::interpretation::departure_evidence::collect(machine, &bytes, &[])?;
    receive(store, machine, id, bytes).await?;
    let evidence = Interpretation {
        departures: Some(departures),
        driver: driver(machine)?,
        config_version: config_version(machine)?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome: Reading::Failed(analyte_model::Reason::new(&reason.to_string())?),
    };
    store
        .request(deadline()?, move |s| {
            s.reconcile(
                analyte_store::ReconcileAction::Interpret(id),
                evidence,
                analyte_store::ReconcileInput::standard(&[], &[]),
                |_, _| Ok(Vec::new()),
            )
        })
        .await?;
    Ok(())
}

/// Retains a parsed POCT Hello as connection context, never as a result transfer.
pub(crate) async fn remember_hello(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
) -> Result<(), Error> {
    persist_mapping(
        store,
        machine,
        id,
        Mapping::Mapped(analyte_mapping::Output::default()),
        None,
    )
    .await?;
    Ok(())
}

pub(crate) async fn failed_reading(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    reason: &str,
) -> Result<(), Error> {
    persist_mapping(
        store,
        machine,
        id,
        Mapping::Parked(ParkReason::Diagnostic(reason.to_owned())),
        None,
    )
    .await?;
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

#[cfg(test)]
mod tests;
