//! LAW program exchanges; all protocol views, builders and ACK rules stay in hl7.
pub(super) mod independent;
use crate::orders::law::{Connection, Reply, response_metadata};
use crate::{Error, evidence, invalid};
use analyte_framing::mllp;
use analyte_hl7::{AckCode, AckError, Options, Responsibility, law};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Cancellation may leave a durable receipt/interpretation, never permission to resend work.
/// The first-byte deadline includes raw commit, interpretation and response issuance.
pub(super) async fn session(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    session_with_outbound(machine, settings, store, conn, stop, None).await
}

async fn session_with_outbound(
    machine: &evidence::Connection<'_>,
    settings: &analyte_config::LawStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
    outbound: Option<&tokio::sync::mpsc::Sender<independent::Work>>,
) -> Result<(), Error> {
    let config = settings;
    let mut options = settings.options();
    options.limits.message_bytes = options.limits.message_bytes.min(config.frame_limit);
    let limit = mllp::Limit::new(options.limits.message_bytes)?;
    let mut decoder = mllp::Decoder::new(limit);
    let receive = Duration::from_millis(u64::from(config.timeouts.receive_timeout_ms));
    let send = Duration::from_millis(u64::from(config.timeouts.send_timeout_ms));
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
                    if outbound.is_none() && opened && decoder.pending().is_empty() {
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
                            orders.issued(Duration::from_millis(u64::from(config.policy.response_ms)))?;
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
                            let mut ready=None;
                            let mut followup_deadline=None;
                            let response=match (response,outbound) {
                                (Some(Reply::Followup {response,work}),Some(sender))=> {
                                    followup_deadline=orders.due();
                                    let (notify,wait)=tokio::sync::oneshot::channel();
                                    let pending=independent::Work {orders:std::mem::replace(&mut orders,Connection::new()?),message:work,ready:wait};
                                    if let Err(error)=sender.try_send(pending) {
                                        orders=error.into_inner().orders;
                                        return Err(invalid("LAW outbound queue unavailable; reserved work not issued"));
                                    }
                                    ready=Some(notify);
                                    Some(Reply::Response(response))
                                }
                                (response,_)=>response,
                            };
                            let (response, has_followup) = match response {
                                None => (Vec::new(), false),
                                Some(Reply::Response(message)) => (vec![message], false),
                                Some(Reply::Followup { response, work }) => (vec![response, work], true),
                            };
                            let write_end = followup_deadline.or_else(||has_followup.then(||orders.due()).flatten()).map(tokio::time::Instant::from_std).map_or(end,|due|due.min(end));
                            for message in response {
                                let send_until = tokio::time::Instant::now().checked_add(send).ok_or("LAW send deadline overflow")?;
                                tokio::select! {
                                    biased;
                                    () = super::stopping(&mut stop) => return Ok(()),
                                    result = write(conn, &message, limit, write_end.min(send_until)) => result?,
                                }
                            }
                            if let Some(ready)=ready { let _ = ready.send(()); }
                            if has_followup { orders.issued(Duration::from_millis(u64::from(config.policy.response_ms)))?; }
                            until = if decoder.pending().is_empty() { None } else { Some(end) };
                        }
                    }
                }
                analyte_transport::Event::Closed(_) => break,
                analyte_transport::Event::Opened { .. } => { opened = true; },
                analyte_transport::Event::Failed(reason) => {
                    tracing::warn!(machine = machine.id.as_str(), error_kind = ?reason, "LAW transport attempt failed");
                },
                analyte_transport::Event::RecordingFailed(_) => {},
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
    machine: &evidence::Connection<'_>,
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
    evidence::receive(store, machine, id, raw.clone()).await?;
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
            let (code, location) = match &error {
                analyte_hl7::Error::Control {
                    segment: "MSH",
                    field,
                } => (
                    AckCode::Ar,
                    u16::try_from(*field)
                        .ok()
                        .map(|field| self::error(b"MSH", field, b"102", b"LAW-control")),
                ),
                analyte_hl7::Error::UnsupportedCharset | analyte_hl7::Error::CharsetMismatch => (
                    AckCode::Ar,
                    Some(self::error(b"MSH", 18, b"102", b"LAW-control")),
                ),
                analyte_hl7::Error::Limit { .. }
                | analyte_hl7::Error::Encoding
                | analyte_hl7::Error::Truncation
                | analyte_hl7::Error::Terminator
                | analyte_hl7::Error::Segment(_)
                | analyte_hl7::Error::Escape { .. }
                | analyte_hl7::Error::CharacterEncoding
                | analyte_hl7::Error::Control { .. }
                | analyte_hl7::Error::NotStored
                | analyte_hl7::Error::AckMode => (AckCode::Ae, None),
            };
            return response_metadata(|response| {
                Ok(law::reject_trigger(
                    payload,
                    response,
                    code,
                    location.unwrap_or(AckError::unlocated(b"102", b"LAW-syntax")),
                    options,
                )?
                .map(Reply::Response))
            });
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
        return response_metadata(|response| {
            Ok(law::reject_trigger(
                payload,
                response,
                AckCode::Ar,
                error(b"MSH", 9, b"200", b"LAW-unsupported-trigger"),
                options,
            )?
            .map(Reply::Response))
        });
    }
    let (code, responsibility, location) = match law::receive_results(&message) {
        Ok(_) => {
            // TF-2b Rev 11 B.7.6 (p.82): inconsistent content rejects the whole batch.
            match evidence::interpret_outcome(store, machine, id).await? {
                evidence::InterpretationOutcome::Accepted => {
                    (AckCode::Aa, Responsibility::StoredAndInterpreted, None)
                }
                evidence::InterpretationOutcome::WorkConflict(location) => (
                    AckCode::Ar,
                    Responsibility::Stored,
                    Some(AckError {
                        segment: location.segment,
                        occurrence: location.occurrence,
                        field: location.field,
                        code: b"207",
                        application_code: b"LAW-inconsistent-work",
                    }),
                ),
                evidence::InterpretationOutcome::Conflict => (
                    AckCode::Ar,
                    Responsibility::Stored,
                    Some(AckError::unlocated(b"207", b"operator-review")),
                ),
                evidence::InterpretationOutcome::Failed => (
                    AckCode::Ae,
                    Responsibility::Stored,
                    Some(AckError::unlocated(b"207", b"operator-review")),
                ),
            }
        }
        Err(failure) => {
            evidence::failed_reading(store, machine, id, &failure.to_string()).await?;
            if law::validate_control(&message, law::ControlDirection::Receive).is_err() {
                let detail = failure.acknowledgment_error(&message, b"102", b"LAW-control");
                return response_metadata(|response| {
                    Ok(law::reject_trigger(
                        payload,
                        response,
                        failure
                            .acknowledgment_code()
                            .ok_or("trigger failure lacks ACK outcome")?,
                        detail,
                        options,
                    )?
                    .map(Reply::Response))
                });
            }
            (
                failure
                    .acknowledgment_code()
                    .ok_or("LAW result failure has no ACK outcome")?,
                Responsibility::Stored,
                Some(failure.acknowledgment_error(&message, b"102", b"LAW-invalid-results")),
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
