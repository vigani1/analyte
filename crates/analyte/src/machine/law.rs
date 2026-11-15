//! LAW program exchanges; all protocol views, builders and ACK rules stay in hl7.
use crate::orders::law::{Connection, Reply, response_metadata};
use crate::{Error, config::Machine, evidence, invalid};
use analyte_framing::mllp;
use analyte_hl7::{AckCode, AckError, Options, Responsibility, law};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Cancellation may leave a durable receipt/interpretation, never permission to resend work.
/// The first-byte deadline includes raw commit, interpretation and response issuance.
pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let config = machine.snapshot.effective().machine();
    let mut options = machine
        .snapshot
        .effective()
        .hl7_options()
        .ok_or("LAW syntax absent")?;
    options.limits.message_bytes = options.limits.message_bytes.min(config.framing.limit);
    let limit = mllp::Limit::new(options.limits.message_bytes)?;
    let mut decoder = mllp::Decoder::new(limit);
    let receive = Duration::from_millis(u64::from(config.dialog.receive_timeout_ms));
    let send = Duration::from_millis(u64::from(config.dialog.send_timeout_ms));
    let mut until = None;
    let mut orders = Connection::new()?;
    let mut opened = false;
    let mut idle = tokio::time::interval(Duration::from_millis(100));
    idle.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let result = async {
        loop {
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut stop) => break,
                () = async { match orders.due() { Some(until) => tokio::time::sleep_until(until.into()).await, None => std::future::pending().await } } => {
                    orders.tick(store).await?;
                    continue;
                },
                () = async { match until { Some(until) => tokio::time::sleep_until(until).await, None => std::future::pending().await } } => {
                    return Err(invalid("LAW receive deadline expired"));
                },
                () = async { idle.tick().await; } => {
                    if opened && decoder.pending().is_empty() {
                        let outgoing = tokio::select! {
                            biased;
                            () = super::stopping(&mut stop) => break,
                            result = orders.idle(store, machine, options) => match result {
                                Ok(message) => message,
                                Err(error) => {
                                    orders.close(store, "LAW-idle-admission-failed").await?;
                                    tracing::error!(machine=machine.id.as_str(),error_kind=%error,"LAW idle admission stopped; no command retried");
                                    None
                                }
                            },
                        };
                        if let Some(message) = outgoing {
                            let end = orders.due().ok_or("LAW outgoing deadline absent")?;
                            tokio::select! {
                                biased;
                                () = super::stopping(&mut stop) => break,
                                result = write(conn, &message, limit, end.into()) => result?,
                            }
                            orders.issued(Duration::from_millis(u64::from(config.law.as_ref().ok_or("LAW options absent")?.response_ms)))?;
                        }
                    }
                    continue;
                },
                event = conn.receive() => match event {
                    Ok(Some(event)) => event,
                    Ok(None) => break,
                    Err(analyte_transport::Error::Deadline(_)) => continue,
                    Err(error) => return Err(error.into()),
                },
            };
            match event {
                analyte_transport::Event::Received { bytes, .. } => {
                    for byte in bytes {
                        let now = tokio::time::Instant::now();
                        let end = *until.get_or_insert(now.checked_add(receive).ok_or("LAW receive deadline overflow")?);
                        if let Some(event) = decoder.push(byte) {
                            let processing_stop = stop.clone();
                            let started = end.into_std().checked_sub(receive).ok_or("LAW first-byte clock underflow")?;
                            let response = tokio::select! {
                                biased;
                                () = super::stopping(&mut stop) => return Ok(()),
                                result = tokio::time::timeout_at(end, process(store, machine, event, options, processing_stop, &mut orders, started)) => result.map_err(|_| invalid("LAW processing deadline expired; no acceptance promised"))??,
                            };
                            let (response, has_followup) = match response {
                                None => (Vec::new(), false),
                                Some(Reply::Response(message)) => (vec![message], false),
                                Some(Reply::Followup { response, work }) => (vec![response, work], true),
                            };
                            let write_end = if has_followup { orders.due().map(tokio::time::Instant::from_std).map_or(end, |due| due.min(end)) } else { end };
                            for message in response {
                                let send_until = tokio::time::Instant::now().checked_add(send).ok_or("LAW send deadline overflow")?;
                                tokio::select! {
                                    biased;
                                    () = super::stopping(&mut stop) => return Ok(()),
                                    result = write(conn, &message, limit, write_end.min(send_until)) => result?,
                                }
                            }
                            if has_followup { orders.issued(Duration::from_millis(u64::from(config.law.as_ref().ok_or("LAW options absent")?.response_ms)))?; }
                            until = if decoder.pending().is_empty() { None } else { Some(end) };
                        }
                    }
                }
                analyte_transport::Event::Closed(_) => break,
                analyte_transport::Event::Opened { .. } => { opened = true; },
                analyte_transport::Event::Failed(_)
                | analyte_transport::Event::RecordingFailed(_) => {},
            }
        }
        Ok(())
    }.await;
    orders
        .close(
            store,
            if result.is_err() {
                "LAW-session-error"
            } else {
                "LAW-disconnect"
            },
        )
        .await?;
    if let Some(mllp::Event::Rejected { raw, error }) = decoder.finish() {
        evidence::park_bytes(store, machine, raw, &error.to_string()).await?;
    }
    result
}

/// A failed or cancelled write may already have issued bytes; the exchange stays owned.
async fn write(
    conn: &mut analyte_transport::Connection,
    message: &analyte_hl7::Message,
    limit: mllp::Limit,
    until: tokio::time::Instant,
) -> Result<(), Error> {
    let bytes = mllp::encode(message.raw(), limit)?;
    tokio::time::timeout_at(until, async {
        for chunk in bytes.chunks(analyte_transport::CHUNK_LIMIT) {
            conn.write(chunk.to_vec()).await?;
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|_| invalid("LAW response send deadline expired"))?
}

async fn process(
    store: &Handle,
    machine: &Machine,
    event: mllp::Event,
    options: Options,
    mut _stop: watch::Receiver<bool>,
    orders: &mut Connection,
    started: std::time::Instant,
) -> Result<Option<Reply>, Error> {
    let raw = match event {
        mllp::Event::Block(raw) => raw,
        mllp::Event::Rejected { raw, error } => {
            evidence::park_bytes(store, machine, raw, &error.to_string()).await?;
            return Err(error.into());
        }
    };
    let id = ReceiptId::new();
    // No parse success or early MSH acceptance can bypass this durable boundary.
    let time = evidence::receive(store, machine, id, raw.clone()).await?;
    #[cfg(feature = "test-hooks")]
    if let Some(hook) = &machine.law_committed {
        hook.pause(&mut _stop).await?;
    }
    let payload = raw
        .get(1..raw.len().saturating_sub(2))
        .ok_or("LAW block boundary")?;
    let message = match law::parse_message(payload, options) {
        Ok(message) => message,
        Err(error) => {
            evidence::failed_reading(store, machine, id, &error.to_string()).await?;
            return Err(error.into());
        }
    };
    let kind: Vec<_> = message
        .segments()
        .next()
        .and_then(|s| s.field(9))
        .map(analyte_hl7::Field::raw)
        .unwrap_or_default()
        .split(|b| Some(b) == message.delimiters().bytes().get(1))
        .take(4)
        .collect();
    if kind == [b"QBP".as_slice(), b"Q11", b"QBP_Q11"] {
        return orders
            .answer(store, machine, &message, id, started, options)
            .await
            .map(Some);
    }
    if kind == [b"ORL".as_slice(), b"O34", b"ORL_O42"] {
        orders.orl(store, machine, &message, id).await?;
        return Ok(None);
    }
    if kind != [b"OUL".as_slice(), b"R22", b"OUL_R22"] {
        // Responses must never start an acknowledgment loop. No LAB-28 is issued here.
        evidence::failed_reading(
            store,
            machine,
            id,
            "LAW transaction disabled: unsolicited work/response traffic is not enabled",
        )
        .await?;
        return Err(invalid(
            "LAW transaction disabled: unsolicited work/response traffic is not enabled",
        ));
    }
    let (code, responsibility, location) = match law::receive_results(&message) {
        Ok(results) => {
            let accepted = evidence::interpret_law(store, machine, id, time, &results).await?;
            if accepted {
                (AckCode::Aa, Responsibility::StoredAndInterpreted, None)
            } else {
                (
                    AckCode::Ae,
                    Responsibility::Stored,
                    Some(AckError::unlocated(b"207", b"operator-review")),
                )
            }
        }
        Err(failure) => {
            evidence::failed_reading(store, machine, id, &failure.to_string()).await?;
            // The pure failure names a segment/field but not its occurrence. Only
            // report a wire location when the original proves it is unambiguous.
            let location = failure.location().filter(|location| {
                location.field > 0
                    && message
                        .segments()
                        .filter(|s| s.id() == location.segment.as_bytes())
                        .count()
                        == 1
            });
            (
                failure
                    .acknowledgment_code()
                    .ok_or("LAW result failure has no ACK outcome")?,
                Responsibility::Stored,
                Some(
                    match location.and_then(|location| {
                        u16::try_from(location.field)
                            .ok()
                            .map(|field| (location, field))
                    }) {
                        Some((location, field)) => error(
                            location.segment.as_bytes(),
                            field,
                            b"102",
                            b"LAW-invalid-results",
                        ),
                        None => AckError::unlocated(b"102", b"LAW-invalid-results"),
                    },
                ),
            )
        }
    };
    response_metadata(|response| {
        Ok(
            law::result_acknowledgment(
                &message,
                response,
                code,
                responsibility,
                location,
                options,
            )?
            .map(Reply::Response),
        )
    })
}

fn error<'a>(
    segment: &'a [u8],
    field: u16,
    code: &'a [u8],
    application_code: &'a [u8],
) -> AckError<'a> {
    AckError {
        segment,
        occurrence: 1,
        field,
        code,
        application_code,
    }
}
