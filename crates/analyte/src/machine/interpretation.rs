//! Interpretation has no transport handle and cannot send orders or acknowledgments.
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_model::{ReceiptId, Revision};
use analyte_store::{Handle, InterpretationCommit, Reprocess};

/// Reinterprets only the retained original bytes using the supplied current machine.
/// Stores an append-only version and exactly one audit, atomically with revision
/// reconciliation and optional parked resolution. No machine connection is opened.
/// Cancellation after submission may leave a committed version; inspect its audit.
/// # Errors
/// Reports missing receipts, machine mismatch, invalid operator evidence and storage
/// failures. Syntax/mapping failures are retained as a new parked interpretation.
pub async fn reprocess(
    store: &Handle,
    machine: &Machine,
    action: Reprocess,
) -> Result<InterpretationCommit, Error> {
    let id = action.receipt;
    let receipt = store
        .request(deadline(), move |s| {
            s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)
        })
        .await?;
    if receipt.metadata.machine != *machine.id() {
        return Err(invalid("receipt belongs to a different machine"));
    }
    let mapping = map(machine, id, receipt.metadata.received_at, &receipt.raw);
    Ok(
        evidence::persist_mapping(store, machine, id, mapping, Some(action))
            .await?
            .1,
    )
}

pub(crate) fn map(
    machine: &Machine,
    id: ReceiptId,
    time: jiff::Timestamp,
    payload: &[u8],
) -> Result<analyte_mapping::Output, Error> {
    let payload = unpack(machine, payload)?;
    let payload = payload.as_ref();
    let effective = machine.snapshot.effective();
    let input = analyte_mapping::Input {
        receipt: id,
        received_at: time,
        revision: Revision(0),
    };
    match effective.machine().syntax.kind {
        analyte_config::SyntaxKind::Poct => {
            let message = analyte_poct::Message::parse(payload, poct_limits(machine)?)?;
            super::vendor_model::poct(machine, id, &message, time)
        }
        analyte_config::SyntaxKind::VidasLiteral => {
            let mut decoder = analyte_vidas::framing::Decoder::new(vidas_config(machine)?);
            let mut packet = None;
            for byte in payload {
                if packet.is_some() {
                    return Err(invalid("VIDAS receipt contains trailing bytes"));
                }
                packet = decoder.push(*byte)?;
            }
            let packet = packet.ok_or_else(|| invalid("VIDAS receipt is incomplete"))?;
            let installed = machine
                .snapshot
                .effective()
                .machine()
                .vidas
                .as_ref()
                .ok_or_else(|| invalid("VIDAS settings missing"))?;
            let message = analyte_vidas::syntax::Message::parse(
                std::str::from_utf8(&packet.body)?,
                match installed.sample_limit {
                    analyte_config::VidasSampleLimit::Twelve => {
                        analyte_vidas::syntax::SampleLimit::Twelve
                    }
                    analyte_config::VidasSampleLimit::Seventeen => {
                        analyte_vidas::syntax::SampleLimit::Seventeen
                    }
                },
            )?;
            super::vendor_model::vidas(machine, id, &message, time)
        }
        analyte_config::SyntaxKind::Autolyser => {
            let policy = effective
                .machine()
                .autolyser
                .as_ref()
                .ok_or_else(|| invalid("Autolyser policy missing"))?;
            let report = analyte_autolyser::records::Report::parse(payload, policy.controls)?;
            super::vendor_model::autolyser(machine, id, &report, time)
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
            if effective.machine().dialog.kind == analyte_config::DialogKind::LawManager {
                let message = analyte_hl7::law::parse_message(
                    payload,
                    effective
                        .hl7_options()
                        .ok_or_else(|| invalid("LAW parser options missing"))?,
                )?;
                let results = analyte_hl7::law::receive_results(&message)?;
                return Ok(analyte_mapping::map_law(
                    &results,
                    &machine.snapshot,
                    input,
                )?);
            }
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

pub(super) fn vidas_config(machine: &Machine) -> Result<analyte_vidas::framing::Config, Error> {
    use analyte_vidas::framing::{Checksum, Config, Mode};
    let settings = machine.snapshot.effective().machine();
    let installed = settings
        .vidas
        .as_ref()
        .ok_or_else(|| invalid("VIDAS settings missing"))?;
    Ok(Config::new(
        if installed.alternate {
            Mode::Alternate
        } else {
            Mode::Literal
        },
        installed.chunk,
        settings.framing.limit,
        &installed.suffix,
        Checksum {
            include_stx: installed.include_stx,
            include_suffix: installed.include_suffix,
            uppercase: installed.uppercase,
            accept_either_case: installed.accept_either_case,
        },
    )?)
}

pub(super) fn poct_limits(machine: &Machine) -> Result<analyte_poct::Limits, Error> {
    let settings = machine.snapshot.effective().machine();
    let policy = settings
        .poct
        .as_ref()
        .ok_or_else(|| invalid("POCT policy missing"))?;
    Ok(analyte_poct::Limits {
        message_bytes: settings.framing.limit.min(settings.syntax.bytes),
        depth: policy.depth,
        elements: policy.elements,
        attribute_bytes: policy.attribute_bytes,
    })
}

#[cfg(test)]
mod tests;

// Stored ASTM receipts contain accepted frames, MLLP receipts contain one block,
// and folder receipts contain syntax bytes. Never infer the envelope from content.
fn unpack<'a>(machine: &Machine, raw: &'a [u8]) -> Result<std::borrow::Cow<'a, [u8]>, Error> {
    use analyte_config::FramingKind;
    let settings = machine.snapshot.effective().machine();
    match settings.framing.kind {
        FramingKind::None
        | FramingKind::Poct
        | FramingKind::VidasLiteral
        | FramingKind::Autolyser => Ok(raw.into()),
        FramingKind::Mllp => {
            use analyte_framing::mllp::{Decoder, Event, Limit};
            let mut decoder = Decoder::new(Limit::new(settings.framing.limit)?);
            let mut complete = false;
            for byte in raw {
                if complete {
                    return Err(invalid(
                        "receipt contains multiple MLLP blocks or trailing bytes",
                    ));
                }
                if let Some(event) = decoder.push(*byte) {
                    match event {
                        Event::Block(_) => complete = true,
                        Event::Rejected { error, .. } => return Err(error.into()),
                    }
                }
            }
            if !complete {
                return Err(invalid("receipt contains incomplete MLLP block"));
            }
            Ok(raw
                .get(
                    1..raw
                        .len()
                        .checked_sub(2)
                        .ok_or_else(|| invalid("MLLP length"))?,
                )
                .ok_or_else(|| invalid("MLLP payload"))?
                .into())
        }
        FramingKind::AstmLis01 => {
            use analyte_framing::astm::{Decoder, Event, HexPolicy, Limit};
            let mut decoder = Decoder::new(Limit::new(settings.framing.limit)?, HexPolicy::Strict);
            let mut payload = Vec::new();
            let mut final_frame = false;
            for byte in raw {
                if let Some(event) = decoder.push(*byte) {
                    match event {
                        Event::Frame(frame) => {
                            if payload
                                .len()
                                .checked_add(frame.text().len())
                                .is_none_or(|n| n > settings.syntax.bytes)
                            {
                                return Err(invalid("ASTM assembled receipt exceeds syntax bound"));
                            }
                            payload.extend_from_slice(frame.text());
                            final_frame = frame.is_final();
                        }
                        Event::Rejected { error, .. } => return Err(error.into()),
                        Event::Outside(_) => {
                            return Err(invalid("receipt contains bytes outside ASTM frames"));
                        }
                    }
                }
            }
            if decoder.finish().is_some() || !final_frame {
                return Err(invalid("receipt contains incomplete ASTM transfer"));
            }
            Ok(payload.into())
        }
    }
}
