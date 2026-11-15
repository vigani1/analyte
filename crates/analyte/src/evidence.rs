//! Protected evidence processing; no payload enters ordinary logs.
use crate::{Error, config::Machine, deadline, invalid};
use analyte_model::{Digest, Identifier, NamespacedId, ReceiptId, ReceiptMetadata, Revision};
use analyte_store::{Handle, Interpretation, Reading, RevisionOutcome};

pub(crate) fn driver(machine: &Machine) -> Result<NamespacedId, Error> {
    Ok(NamespacedId {
        namespace: Identifier::new(match machine.snapshot.effective().machine().syntax.kind {
            analyte_config::SyntaxKind::AstmLis02 => "analyte-astm",
            analyte_config::SyntaxKind::Hl7V2 => "analyte-hl7",
            analyte_config::SyntaxKind::Poct => "analyte-poct",
            analyte_config::SyntaxKind::VidasLiteral => "analyte-vidas",
            analyte_config::SyntaxKind::Autolyser => "analyte-autolyser",
        })?,
        value: Identifier::new(env!("CARGO_PKG_VERSION"))?,
    })
}

pub(crate) async fn receive(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    raw: Vec<u8>,
) -> Result<jiff::Timestamp, Error> {
    let received_at = jiff::Timestamp::now();
    let config = machine.snapshot.canonical().as_bytes().to_vec();
    let metadata = ReceiptMetadata {
        id,
        raw_digest: Digest(analyte_contract::content_digest(&raw)),
        connection: machine.id.clone(),
        machine: machine.id.clone(),
        driver: driver(machine)?,
        config_version: Identifier::new("1")?,
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

fn mapped(
    machine: &Machine,
    id: ReceiptId,
    time: jiff::Timestamp,
    payload: &[u8],
) -> Result<analyte_mapping::Output, Error> {
    let effective = machine.snapshot.effective();
    let input = analyte_mapping::Input {
        receipt: id,
        received_at: time,
        revision: Revision(0),
    };
    match effective.machine().syntax.kind {
        analyte_config::SyntaxKind::Poct
        | analyte_config::SyntaxKind::VidasLiteral
        | analyte_config::SyntaxKind::Autolyser => {
            Err(invalid("vendor syntax bypasses field mapping"))
        }
        analyte_config::SyntaxKind::AstmLis02 => {
            let limits = effective
                .astm_limits()
                .ok_or_else(|| invalid("ASTM parser limits missing"))?;
            let messages = analyte_astm::parse_with(payload, limits, |bytes| {
                match effective.machine().syntax.encoding {
                    analyte_config::Encoding::Ascii => bytes
                        .is_ascii()
                        .then(|| String::from_utf8_lossy(bytes).into_owned()),
                    analyte_config::Encoding::Latin1 => analyte_astm::latin1(bytes),
                    analyte_config::Encoding::Utf8 => {
                        std::str::from_utf8(bytes).ok().map(str::to_owned)
                    }
                }
            })?;
            // A transfer may contain multiple messages; the host must retain every one.
            if messages.len() != 1 {
                return Err(invalid(
                    "ASTM transfer does not contain exactly one message; operator review required",
                ));
            }
            Ok(analyte_mapping::map_astm(
                messages
                    .first()
                    .ok_or_else(|| invalid("ASTM message missing"))?,
                &machine.snapshot,
                input,
            )?)
        }
        analyte_config::SyntaxKind::Hl7V2 => {
            let message = analyte_hl7::Message::parse(
                payload,
                effective
                    .hl7_options()
                    .ok_or_else(|| invalid("HL7 parser options missing"))?,
            )?;
            Ok(analyte_mapping::map_hl7(
                &message,
                &machine.snapshot,
                input,
            )?)
        }
    }
}

pub(crate) async fn interpret(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    time: jiff::Timestamp,
    payload: &[u8],
) -> Result<bool, Error> {
    persist_mapping(store, machine, id, mapped(machine, id, time, payload)).await
}

pub(crate) async fn interpret_law(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    time: jiff::Timestamp,
    results: &analyte_hl7::law::Results<'_>,
) -> Result<bool, Error> {
    let mapped = analyte_mapping::map_law(
        results,
        &machine.snapshot,
        analyte_mapping::Input {
            receipt: id,
            received_at: time,
            revision: Revision(0),
        },
    )
    .map_err(Into::into);
    persist_mapping(store, machine, id, mapped).await
}

async fn persist_mapping(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    mapping: Result<analyte_mapping::Output, Error>,
) -> Result<bool, Error> {
    let atomic = machine.snapshot.effective().machine().law.is_some();
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
    let interpretation = Interpretation {
        driver: driver(machine)?,
        config_version: Identifier::new("1")?,
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
            let findings = findings
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
                .collect::<Result<Vec<_>, analyte_store::Error>>()?;
            let receipt = store
                .receipt(id)?
                .ok_or(analyte_store::Error::Conflict)?
                .metadata;
            let commit = store.interpret_reconciled(
                id,
                interpretation,
                &corrections,
                &findings,
                |assigned, outcomes| {
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
                    build_deliveries(assigned, outcomes, &receipt, &destination)
                },
            );
            match commit {
                // The builder refusal rolls back revisions and deliveries together.
                // Retain a failed interpretation on the original receipt afterwards.
                Err(analyte_store::Error::DeliveryBuild(error))
                    if error.is::<LawBatchConflict>() =>
                {
                    let rejected = store.interpret_reconciled(
                        id,
                        conflict.ok_or(analyte_store::Error::InvalidInput)?,
                        &[],
                        &findings,
                        |_, _| Ok(Vec::new()),
                    )?;
                    Ok((rejected, false))
                }
                result => result.map(|commit| (commit, accepted)),
            }
        })
        .await?;
    Ok(result.1
        && !result
            .0
            .outcomes
            .iter()
            .any(|o| matches!(o, RevisionOutcome::Conflict | RevisionOutcome::Unresolved)))
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
            if fresh && result.processing.may_deliver_to_production() && !result.requires_parking()
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
    machine: &Machine,
    bytes: Vec<u8>,
    reason: &str,
) -> Result<(), Error> {
    let id = ReceiptId::new();
    receive(store, machine, id, bytes).await?;
    let evidence = Interpretation {
        driver: driver(machine)?,
        config_version: Identifier::new("1")?,
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
    persist_mapping(store, machine, id, Ok(output)).await?;
    Ok(())
}

pub(crate) async fn failed_reading(
    store: &Handle,
    machine: &Machine,
    id: ReceiptId,
    reason: &str,
) -> Result<(), Error> {
    persist_mapping(store, machine, id, Err(invalid(reason))).await?;
    Ok(())
}
