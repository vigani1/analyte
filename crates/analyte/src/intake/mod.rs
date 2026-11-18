//! Stores receipts, maps results, commits interpretations, revisions and deliveries, and parks failures.
use crate::{deadline, installation::config::Machine};

mod error;
use crate::machine::interpretation::{
    policy::{BatchConflict, Policy, WorkMismatch},
    retained,
};
pub(crate) use crate::machine::receive::bytes::ByteReason;
use analyte_model::{Digest, Identifier, NamespacedId, ReceiptId, ReceiptMetadata};
use analyte_store::{Handle, Interpretation, Reading, RevisionOutcome};
pub use error::IntakeError as Error;
use std::fmt::Write;
mod parking;
mod reading;

pub(crate) use crate::machine::interpretation::map_reading;
pub(crate) use parking::{Mapping, ParkReason};
use reading::mapping_outcome;
mod persistence;
pub use persistence::CommitError;
use persistence::PreparedMapping;

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
    WorkConflict(WorkMismatch),
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
        Option<crate::machine::hl7::location::Location>,
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

pub(crate) async fn persist_prepared(
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
    let mut policy = context.policy(store, machine).await?;
    let atomic = policy.atomic();
    let work_conflict = policy.work_conflict();
    let mapping = if let Some(reason) = policy.conflict_reason()? {
        Mapping::Parked(reason)
    } else {
        mapping
    };
    let MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    } = prepare_reading(map_reading(mapping)?, &policy)?;
    if !accepted && !work_conflict {
        policy.discard_custody();
    }
    let interpretation = reading_evidence(machine, reading, departures)?;
    let destination = machine.snapshot.effective().site().destination.name.clone();
    let conflict = if atomic {
        Some(Interpretation {
            departures: interpretation.departures.clone(),
            driver: interpretation.driver.clone(),
            config_version: interpretation.config_version.clone(),
            config: interpretation.config.clone(),
            outcome: Reading::Failed(analyte_model::Reason::new(&BatchConflict.to_string())?),
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
        policy,
        action,
        accepted,
    };
    let result = store
        .request(deadline()?, move |store| prepared.commit(store))
        .await?;
    let (result, successful) = result;
    mapping_outcome(result, successful)
}

fn prepare_reading(mapped: MappedReading, policy: &Policy) -> Result<MappedReading, Error> {
    let MappedReading {
        accepted,
        reading,
        corrections,
        findings,
    } = mapped;
    // A rejected atomic batch must not register otherwise-new history without an
    // outbox delivery: a later accepted retry would be mistaken for a retransmission.
    let (reading, corrections) = if let Some(reason) = policy.mapping_refusal()
        && !accepted
        && matches!(reading, Reading::Results(_))
    {
        (
            Reading::Failed(analyte_model::Reason::new(reason)?),
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

/// Records a handled non-result receipt (a POCT greeting kept as connection context,
/// an answered LAW query or work response) as interpreted with no results, never as a
/// result transfer. Startup recovery then sees it as done. Call it only after the
/// handling step commits, so a crash before this call leaves the receipt pending.
/// A receipt an earlier step already parked with its own reason keeps that reason.
pub(crate) async fn mark_handled(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
) -> Result<(), Error> {
    let interpreted = store
        .request(deadline()?, move |s| s.interpretation(id, 1))
        .await?
        .is_some();
    if interpreted {
        return Ok(());
    }
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

pub(crate) struct MappedReading {
    pub(crate) accepted: bool,
    pub(crate) reading: Reading,
    pub(crate) corrections: Vec<bool>,
    pub(crate) findings: Vec<String>,
}
