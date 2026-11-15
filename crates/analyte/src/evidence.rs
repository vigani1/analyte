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
    let mapping = mapped(machine, id, time, payload);
    let mut accepted = false;
    let (reading, corrections, findings) = match mapping {
        Ok(output) => {
            accepted = !output.requires_parking();
            let findings = output
                .findings
                .iter()
                .map(|f| format!("{:?}: {:?}", f.position, f.problem))
                .collect::<Vec<_>>();
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
    let interpretation = Interpretation {
        driver: driver(machine)?,
        config_version: Identifier::new("1")?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome: reading,
    };
    let destination = machine.snapshot.effective().site().destination.name.clone();
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
            store.interpret_reconciled(
                id,
                interpretation,
                &corrections,
                &findings,
                |assigned, outcomes| {
                    if !accepted {
                        return Ok(Vec::new());
                    }
                    build_deliveries(assigned, outcomes, &receipt, &destination)
                },
            )
        })
        .await?;
    Ok(accepted
        && !result
            .outcomes
            .iter()
            .any(|o| matches!(o, RevisionOutcome::Conflict | RevisionOutcome::Unresolved)))
}

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
    reading: Reading,
) -> Result<(), Error> {
    let corrections = match &reading {
        Reading::Results(results) => vec![false; results.len()],
        Reading::Failed(_) => Vec::new(),
    };
    let mut findings = Vec::new();
    if let Reading::Results(results) = &reading {
        for result in results {
            if let analyte_model::IdentityStatus::Unresolved(reason) = &result.identity {
                findings.push(analyte_store::Parked {
                    kind: Identifier::new("unresolved_identity")?,
                    reason: reason.clone(),
                    targets: vec![Identifier::new(&uuid::Uuid::from(id).to_string())?],
                });
            }
        }
    }
    let mut unknown_test = false;
    if let Reading::Results(results) = &reading {
        for result in results {
            for part in result.parts.as_slice() {
                if !machine
                    .snapshot
                    .effective()
                    .machine()
                    .tests()
                    .iter()
                    .any(|entry| {
                        entry.app_system == part.test.namespace && entry.app_code == part.test.value
                    })
                {
                    unknown_test = true;
                }
            }
        }
    }
    if unknown_test {
        findings.push(analyte_store::Parked {
            kind: Identifier::new("unknown_vendor_test")?,
            reason: analyte_model::Reason::new("vendor test has no unique configured translation")?,
            targets: vec![Identifier::new(&uuid::Uuid::from(id).to_string())?],
        });
    }
    let interpretation = Interpretation {
        driver: driver(machine)?,
        config_version: Identifier::new("1")?,
        config: machine.snapshot.canonical().as_bytes().to_vec(),
        outcome: reading,
    };
    let destination = machine.snapshot.effective().site().destination.name.clone();
    store
        .request(deadline(), move |store| {
            let receipt = store
                .receipt(id)?
                .ok_or(analyte_store::Error::Conflict)?
                .metadata;
            store.interpret_reconciled(
                id,
                interpretation,
                &corrections,
                &findings,
                |assigned, outcomes| {
                    if unknown_test {
                        return Ok(Vec::new());
                    }
                    build_deliveries(assigned, outcomes, &receipt, &destination)
                },
            )
        })
        .await?;
    Ok(())
}
