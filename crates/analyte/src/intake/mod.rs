//! Stores receipts, maps results, commits interpretations, revisions and deliveries, and parks failures.
use crate::{deadline, installation::config::Machine};
mod bytes;
mod error;
use crate::machine::law::custody as law;
use analyte_model::{Digest, Identifier, NamespacedId, ReceiptId, ReceiptMetadata};
use analyte_store::{Handle, Interpretation, Reading, RevisionOutcome};
pub(crate) use bytes::ByteReason;
pub use error::IntakeError as Error;
use std::fmt::Write;
mod parking;
mod reading;
pub(crate) mod retained;
pub(crate) use parking::{Mapping, ParkReason};
use reading::{map_reading, mapping_outcome};
mod persistence;
pub use persistence::CommitError;
use persistence::{LawBatchConflict, PreparedMapping};

/// Binds all evidence from one host connection to its immutable configuration.
pub(crate) struct ConnectionEvidence<'a> {
    machine: &'a Machine,
    connection_id: Identifier,
}

impl<'a> ConnectionEvidence<'a> {
    pub(crate) fn new(machine: &'a Machine) -> Result<Self, Error> {
        Ok(Self {
            machine,
            connection_id: Identifier::new(&uuid::Uuid::now_v7().to_string())?,
        })
    }
}

impl ConnectionEvidence<'_> {
    pub(crate) fn machine(&self) -> &Machine {
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
    machine: &ConnectionEvidence<'_>,
    id: ReceiptId,
    raw: Vec<u8>,
) -> Result<jiff::Timestamp, Error> {
    let received_at = jiff::Timestamp::now();
    let config = machine.machine().snapshot.canonical().as_bytes().to_vec();
    let metadata = ReceiptMetadata {
        id,
        raw_digest: Digest(analyte_contract::content_digest(&raw)),
        connection: machine.connection_id.clone(),
        machine: machine.machine.id.clone(),
        driver: driver(machine.machine())?,
        config_version: config_version(machine.machine())?,
        config_digest: Digest(machine.machine().snapshot.digest()),
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
            s.receipt(id)?.ok_or(Error::UnknownReceipt)
        })
        .await?;
    let (outcome, _, location) = retained::process(store, machine, &receipt, None, None).await?;
    Ok((outcome, location))
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

async fn persist_prepared(
    store: &Handle,
    machine: &Machine,
    context: &retained::Context<'_>,
    mapping: Mapping,
    action: Option<analyte_store::Reprocess>,
) -> Result<(InterpretationOutcome, analyte_store::InterpretationCommit), Error> {
    let id = context.receipt.metadata.id;
    let (mapping, departures) = super::machine::interpretation::departure_evidence::for_receipt(
        machine,
        context.receipt,
        context.wire.as_ref(),
        mapping,
    )?;
    let poct_edit_guard = poct_edits(machine);
    let atomic = atomic_batch(machine);
    let lifecycle = atomic.then_some(law_lifecycle());
    let work_conflict = law::work_conflict(store, machine, context.results).await?;
    let mut custody = law::plan(store, machine, context.results, work_conflict).await?;
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
        Some(analyte_store::law::Disposition::Accepted) => InterpretationOutcome::Accepted,
        Some(analyte_store::law::Disposition::Rejected(rejection)) => {
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
    machine.snapshot.effective().stack().atomic_batches()
}

fn park_findings(
    id: ReceiptId,
    findings: Vec<String>,
) -> Result<Vec<analyte_store::Parked>, analyte_model::TextError> {
    findings
        .into_iter()
        .map(|finding| {
            Ok(analyte_store::Parked {
                kind: Identifier::new("mapping_finding")?,
                reason: analyte_model::Reason::new(&finding)?,
                targets: vec![Identifier::new(&uuid::Uuid::from(id).to_string())?],
            })
        })
        .collect::<Result<Vec<_>, analyte_model::TextError>>()
}

pub(crate) async fn park_bytes(
    store: &Handle,
    machine: &ConnectionEvidence<'_>,
    bytes: Vec<u8>,
    reason: ByteReason<'_>,
) -> Result<(), Error> {
    let id = ReceiptId::new();
    let departures = super::machine::interpretation::departure_evidence::collect(
        machine.machine(),
        &bytes,
        &[],
    )?;
    receive(store, machine, id, bytes).await?;
    let evidence = Interpretation {
        departures: Some(departures),
        driver: driver(machine.machine())?,
        config_version: config_version(machine.machine())?,
        config: machine.machine().snapshot.canonical().as_bytes().to_vec(),
        outcome: Reading::Failed(analyte_model::Reason::new(&reason.to_string())?),
    };
    store
        .request::<_, analyte_store::ReconciliationError>(deadline()?, move |s| {
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

async fn persist_mapping(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    mapping: Mapping,
    action: Option<analyte_store::Reprocess>,
) -> Result<(), Error> {
    let receipt = store
        .request(deadline()?, move |s| {
            s.receipt(id)?.ok_or(Error::UnknownReceipt)
        })
        .await?;
    retained::process(store, machine, &receipt, Some(mapping), action).await?;
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
