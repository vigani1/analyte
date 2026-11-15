//! LAB-29 responder (TF-2b 3.29.4/B.7); all protocol views and ACK rules stay in hl7.
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
    let result = async {
        loop {
            let event = tokio::select! {
                biased;
                () = super::stopping(&mut stop) => break,
                () = async { match until { Some(until) => tokio::time::sleep_until(until).await, None => std::future::pending().await } } => {
                    return Err(invalid("LAW receive deadline expired"));
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
                            let response = tokio::select! {
                                biased;
                                () = super::stopping(&mut stop) => return Ok(()),
                                result = tokio::time::timeout_at(end, process(store, machine, event, options, processing_stop)) => result.map_err(|_| invalid("LAW processing deadline expired; no acceptance promised"))??,
                            };
                            if let Some(message) = response {
                                let bytes = mllp::encode(message.raw(), limit)?;
                                let send_until = tokio::time::Instant::now().checked_add(send).ok_or("LAW send deadline overflow")?;
                                tokio::select! {
                                    biased;
                                    () = super::stopping(&mut stop) => return Ok(()),
                                    result = tokio::time::timeout_at(end.min(send_until), async {
                                        for chunk in bytes.chunks(analyte_transport::CHUNK_LIMIT) { conn.write(chunk.to_vec()).await?; }
                                        Ok::<_, Error>(())
                                    }) => result.map_err(|_| invalid("LAW response send deadline expired"))??,
                                }
                            }
                            until = if decoder.pending().is_empty() { None } else { Some(end) };
                        }
                    }
                }
                analyte_transport::Event::Closed(_) => break,
                analyte_transport::Event::Opened { .. }
                | analyte_transport::Event::Failed(_)
                | analyte_transport::Event::RecordingFailed(_) => {},
            }
        }
        Ok(())
    }.await;
    if let Some(mllp::Event::Rejected { raw, error }) = decoder.finish() {
        evidence::park_bytes(store, machine, raw, &error.to_string()).await?;
    }
    result
}

async fn process(
    store: &Handle,
    machine: &Machine,
    event: mllp::Event,
    options: Options,
    mut _stop: watch::Receiver<bool>,
) -> Result<Option<analyte_hl7::Message>, Error> {
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
    if kind != [b"OUL".as_slice(), b"R22", b"OUL_R22"] {
        // Responses must never start an acknowledgment loop. No LAB-28 is issued here.
        evidence::failed_reading(
            store,
            machine,
            id,
            "LAW transaction disabled: only LAB-29 results are enabled",
        )
        .await?;
        if kind == [b"QBP".as_slice(), b"Q11", b"QBP_Q11"] {
            let query = law::receive_query(&message)?;
            return response_metadata(|response| {
                Ok(Some(
                    law::query_response(
                        query,
                        law::CacheAnswer::Unavailable(AckError::unlocated(
                            b"207",
                            b"LAW-LAB27-disabled",
                        )),
                        response,
                        Responsibility::Stored,
                        options,
                    )?
                    .message()
                    .clone(),
                ))
            });
        }
        return Err(invalid(
            "LAW transaction disabled: only LAB-29 results are enabled",
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
        Ok(law::result_acknowledgment(
            &message,
            response,
            code,
            responsibility,
            location,
            options,
        )?)
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

fn response_metadata<T>(
    build: impl FnOnce(analyte_hl7::Response<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let id = uuid::Uuid::now_v7().simple().to_string();
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y%m%d%H%M%S%z")
        .to_string();
    build(analyte_hl7::Response {
        control_id: law::control_id(id.as_bytes())?,
        timestamp: timestamp.as_bytes(),
    })
}
