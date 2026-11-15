//! Interpretation has no transport handle and cannot send orders or acknowledgments.
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_config::Stack;
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
    let mapping = map_retained(store, machine, &receipt).await;
    Ok(
        evidence::persist_mapping(store, machine, id, mapping, Some(action))
            .await?
            .1,
    )
}

// Recover the source Hello from durable connection evidence for live receive and replay alike.
pub(crate) async fn map_retained(
    store: &Handle,
    machine: &Machine,
    receipt: &analyte_store::Receipt,
) -> Result<analyte_mapping::Output, Error> {
    if let Stack::Poct(settings) = machine.snapshot.effective().stack() {
        let machine_id = receipt.metadata.machine.clone();
        let connection = receipt.metadata.connection.clone();
        let first = store
            .request(deadline(), move |s| {
                s.first_interpreted_connection_receipt(&machine_id, &connection)
            })
            .await?;
        let hello = first
            .as_ref()
            .map(|first| analyte_poct::Message::parse(&first.raw, poct_limits(settings)))
            .transpose()?;
        let device = hello.as_ref().and_then(|hello| match hello.body() {
            analyte_poct::Body::Hello { device_id, .. } => Some(device_id.as_str()),
            analyte_poct::Body::Status { .. }
            | analyte_poct::Body::PatientObservations(_)
            | analyte_poct::Body::NonPatientObservations(_)
            | analyte_poct::Body::EndTopic(_)
            | analyte_poct::Body::Terminate(_)
            | analyte_poct::Body::Acknowledgement { .. }
            | analyte_poct::Body::Escape { .. }
            | analyte_poct::Body::Request(_) => None,
        });
        let message = analyte_poct::Message::parse(&receipt.raw, poct_limits(settings))?;
        return Ok(analyte_mapping::map_poct(
            &machine.snapshot,
            receipt.metadata.id,
            &message,
            receipt.metadata.received_at,
            device,
        )?);
    }
    map(
        machine,
        receipt.metadata.id,
        receipt.metadata.received_at,
        &receipt.raw,
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
    match effective.stack() {
        Stack::Poct(settings) => {
            let message = analyte_poct::Message::parse(payload, poct_limits(settings))?;
            Ok(analyte_mapping::map_poct(
                &machine.snapshot,
                id,
                &message,
                time,
                None,
            )?)
        }
        Stack::VidasLiteral(settings) => {
            let mut decoder = analyte_framing::vidas::Decoder::new(vidas_config(machine)?);
            let mut packet = None;
            for byte in payload {
                if packet.is_some() {
                    return Err(invalid("VIDAS receipt contains trailing bytes"));
                }
                packet = decoder.push(*byte)?;
            }
            let packet = packet.ok_or_else(|| invalid("VIDAS receipt is incomplete"))?;
            let message = analyte_vidas::syntax::Message::parse(
                std::str::from_utf8(&packet.body)?,
                match settings.policy().sample_limit {
                    analyte_config::VidasSampleLimit::Twelve => {
                        analyte_vidas::syntax::SampleLimit::Twelve
                    }
                    analyte_config::VidasSampleLimit::Seventeen => {
                        analyte_vidas::syntax::SampleLimit::Seventeen
                    }
                },
            )?;
            Ok(analyte_mapping::map_vidas(
                &machine.snapshot,
                id,
                &message,
                time,
            )?)
        }
        Stack::Autolyser(settings) => {
            let report =
                analyte_autolyser::syntax::Report::parse(payload, settings.policy().controls)?;
            Ok(analyte_mapping::map_autolyser(
                &machine.snapshot,
                id,
                &report,
                time,
            )?)
        }
        Stack::Astm(settings) => map_astm(machine, payload, input, settings.syntax()),
        Stack::FileAstm(settings) => map_astm(machine, payload, input, settings.syntax()),
        Stack::Hl7(settings) => map_hl7(machine, payload, input, settings.syntax()),
        Stack::FileHl7(settings) => map_hl7(machine, payload, input, settings.syntax()),
        Stack::Law(settings) => {
            let message = analyte_hl7::law::parse_message(payload, settings.options())?;
            let results = analyte_hl7::law::receive_results(&message)?;
            Ok(analyte_mapping::map_law(
                &results,
                &machine.snapshot,
                input,
            )?)
        }
    }
}

fn map_astm(
    machine: &Machine,
    payload: &[u8],
    input: analyte_mapping::Input,
    syntax: &analyte_config::AstmSyntax,
) -> Result<analyte_mapping::Output, Error> {
    let messages = analyte_astm::parse_with(payload, syntax.parser_limits(), |bytes| match syntax
        .encoding()
    {
        analyte_config::Encoding::Ascii => bytes
            .is_ascii()
            .then(|| String::from_utf8_lossy(bytes).into_owned()),
        analyte_config::Encoding::Latin1 => analyte_astm::latin1(bytes),
        analyte_config::Encoding::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_owned),
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

fn map_hl7(
    machine: &Machine,
    payload: &[u8],
    input: analyte_mapping::Input,
    syntax: &analyte_config::Hl7Syntax,
) -> Result<analyte_mapping::Output, Error> {
    let message = analyte_hl7::Message::parse(payload, syntax.options())?;
    Ok(analyte_mapping::map_hl7(
        &message,
        &machine.snapshot,
        input,
    )?)
}

pub(super) fn vidas_config(machine: &Machine) -> Result<analyte_framing::vidas::Config, Error> {
    use analyte_framing::vidas::{Checksum, Config, Mode};
    let settings = match machine.snapshot.effective().stack() {
        Stack::VidasLiteral(settings) => settings,
        Stack::Astm(_)
        | Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Autolyser(_)
        | Stack::FileAstm(_)
        | Stack::FileHl7(_) => return Err(invalid("VIDAS settings missing")),
    };
    let installed = &settings.policy();
    Ok(Config::new(
        if installed.alternate {
            Mode::Alternate
        } else {
            Mode::Literal
        },
        installed.chunk,
        settings.frame_limit(),
        &installed.suffix,
        Checksum {
            include_stx: installed.include_stx,
            include_suffix: installed.include_suffix,
            uppercase: installed.uppercase,
            accept_either_case: installed.accept_either_case,
        },
    )?)
}

pub(super) fn poct_limits(settings: &analyte_config::PoctStack) -> analyte_poct::Limits {
    let policy = &settings.policy();
    analyte_poct::Limits {
        message_bytes: settings.frame_limit().min(settings.limits().bytes()),
        depth: policy.depth,
        elements: policy.elements,
        attribute_bytes: policy.attribute_bytes,
    }
}

#[cfg(test)]
mod tests;

// Stored ASTM receipts contain accepted frames, MLLP receipts contain one block,
// and folder receipts contain syntax bytes. Never infer the envelope from content.
pub(crate) fn unpack<'a>(
    machine: &Machine,
    raw: &'a [u8],
) -> Result<std::borrow::Cow<'a, [u8]>, Error> {
    match machine.snapshot.effective().stack() {
        Stack::FileAstm(_)
        | Stack::FileHl7(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_) => Ok(raw.into()),
        Stack::Hl7(settings) => unpack_mllp(raw, settings.frame_limit()),
        Stack::Law(settings) => unpack_mllp(raw, settings.frame_limit()),
        Stack::Astm(settings) => {
            use analyte_framing::astm::{Decoder, Event, HexPolicy, Limit};
            let mut decoder = Decoder::new(Limit::new(settings.frame_limit())?, HexPolicy::Strict);
            let mut payload = Vec::new();
            let mut final_frame = false;
            for byte in raw {
                if let Some(event) = decoder.push(*byte) {
                    match event {
                        Event::Frame(frame) => {
                            if payload
                                .len()
                                .checked_add(frame.text().len())
                                .is_none_or(|n| n > settings.syntax().limits().bytes())
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

fn unpack_mllp(raw: &[u8], frame_limit: usize) -> Result<std::borrow::Cow<'_, [u8]>, Error> {
    use analyte_framing::mllp::{Decoder, Event, Limit};
    let mut decoder = Decoder::new(Limit::new(frame_limit)?);
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
    Ok(analyte_framing::mllp::payload(raw)
        .ok_or_else(|| invalid("MLLP payload"))?
        .into())
}
