use super::{
    Code, Control, Error, Header, Order, OrderId, OrderMessage, SampleField, Subject, invalid,
    types::scalar,
};
use crate::{Charset, Message, Options, Terminator, Version};
use std::collections::BTreeSet;

#[derive(Default)]
struct IdentitySet<'a> {
    local: BTreeSet<(&'a [u8], &'a [u8])>,
    universal: BTreeSet<(&'a [u8], &'a [u8], &'a [u8])>,
}

impl<'a> IdentitySet<'a> {
    fn insert(&mut self, id: OrderId<'a>, field: usize) -> Result<(), Error> {
        // Adding universal metadata cannot make a repeated local ID new work.
        if !id.namespace.is_empty() && !self.local.insert((id.id, id.namespace)) {
            return Err(Error::Duplicate { field });
        }
        if let Some((universal, kind)) = id.universal
            && !self.universal.insert((id.id, universal, kind))
        {
            return Err(Error::Duplicate { field });
        }
        Ok(())
    }
}

fn append(output: &mut Vec<u8>, value: &[u8], maximum: usize) -> Result<(), Error> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|n| n > maximum)
    {
        return Err(Error::Limit);
    }
    output.extend_from_slice(value);
    Ok(())
}

fn segment(
    output: &mut Vec<u8>,
    name: &[u8],
    fields: &[&[u8]],
    maximum: usize,
) -> Result<(), Error> {
    append(output, name, maximum)?;
    for value in fields {
        append(output, b"|", maximum)?;
        append(output, value, maximum)?;
    }
    append(output, b"\r", maximum)
}

fn escape(value: &[u8], segment: &str, field: usize, maximum: usize) -> Result<Vec<u8>, Error> {
    let mut encoded = Vec::new();
    for b in value {
        let replacement: &[u8] = match b {
            b'|' => b"\\F\\",
            b'^' => b"\\S\\",
            b'~' => b"\\R\\",
            b'\\' => b"\\E\\",
            b'&' => b"\\T\\",
            _ => std::slice::from_ref(b),
        };
        if encoded
            .len()
            .checked_add(replacement.len())
            .is_none_or(|n| n > maximum)
        {
            return Err(invalid(segment, field));
        }
        encoded.extend_from_slice(replacement);
    }
    Ok(encoded)
}

fn identity(id: OrderId<'_>, version: Version, field: usize) -> Result<Vec<u8>, Error> {
    let mut raw = id.id.to_vec();
    raw.push(b'^');
    raw.extend_from_slice(id.namespace);
    if let Some((identifier, kind)) = id.universal {
        if !crate::datatypes::uid(kind, version) {
            return Err(invalid("ORC", field));
        }
        raw.push(b'^');
        raw.extend_from_slice(identifier);
        raw.push(b'^');
        raw.extend_from_slice(kind);
    }
    Ok(raw)
}

fn code(code: Code<'_>, separator: u8, maximum: usize, field: usize) -> Result<Vec<u8>, Error> {
    let mut raw = code.identifier.to_vec();
    raw.push(separator);
    raw.extend_from_slice(code.text);
    raw.push(separator);
    raw.extend_from_slice(code.system);
    if raw.len() > maximum {
        return Err(invalid("OBR", field));
    }
    Ok(raw)
}

/// Builds a bounded original-mode ORM/O01 from explicit caller-authorized work.
///
/// Each new order has one OBR test/battery; multiple tests require distinct placer
/// identities. A cancellation has no invented OBR/sample/test details. Sample
/// placement in a placer-owned field must be agreed in the machine configuration.
/// No transmission, durable storage, retry or lifecycle transition occurs here.
///
/// # Errors
/// Rejects empty work, duplicate identities, invalid metadata, unsupported values
/// or an encoded message exceeding the caller's limits.
pub fn orm(
    orders: &[Order<'_>],
    subject: Subject<'_>,
    header: Header<'_>,
    options: Options,
) -> Result<OrderMessage, Error> {
    if orders.is_empty() {
        return Err(invalid("ORC", 0));
    }
    if orders.len() > options.limits.segments {
        return Err(Error::Limit);
    }
    for (value, field, required) in [
        (header.application, 3, true),
        (header.facility, 4, false),
        (header.peer_application, 5, true),
        (header.peer_facility, 6, false),
        (header.control_id.raw(), 10, true),
    ] {
        scalar(value, "MSH", field, 20, required)?;
    }
    let mut output = Vec::new();
    let version = match header.version {
        Version::V231 => b"2.3.1".as_slice(),
        Version::V251 => b"2.5.1",
    };
    let charset = match options.charset {
        Charset::Ascii => b"ASCII".as_slice(),
        Charset::Latin1 => b"8859/1",
        Charset::Utf8 => b"UNICODE UTF-8",
    };
    segment(
        &mut output,
        b"MSH",
        &[
            b"^~\\&",
            header.application,
            header.facility,
            header.peer_application,
            header.peer_facility,
            header.timestamp.raw(),
            b"",
            b"ORM^O01^ORM_O01",
            header.control_id.raw(),
            header.processing.wire(),
            version,
            b"",
            b"",
            b"",
            b"",
            b"",
            charset,
        ],
        options.limits.message_bytes,
    )?;
    match subject {
        Subject::NonPatient => {}
        Subject::Patient(patient) => {
            scalar(patient.identifier, "PID", 3, 15, true)?;
            scalar(patient.namespace, "PID", 3, 20, true)?;
            if patient.family_name.is_empty()
                || patient.family_name == b"\"\""
                || patient.family_name.len() > 48
                || patient.family_name.iter().any(u8::is_ascii_control)
            {
                return Err(invalid("PID", 5));
            }
            let mut identifier = patient.identifier.to_vec();
            identifier.extend_from_slice(b"^^^");
            identifier.extend_from_slice(patient.namespace);
            if header.version == Version::V231 && identifier.len() > 20 {
                return Err(invalid("PID", 3));
            }
            let name = escape(patient.family_name, "PID", 5, 48)?;
            segment(
                &mut output,
                b"PID",
                &[b"1", b"", &identifier, b"", &name],
                options.limits.message_bytes,
            )?;
        }
    }
    let mut placer_ids = IdentitySet::default();
    let mut filler_ids = IdentitySet::default();
    for (offset, order) in orders.iter().enumerate() {
        let (placer, filler, control) = match order {
            Order::New { placer, filler, .. } => (*placer, *filler, Control::New),
            Order::Cancel { placer, filler } => (*placer, *filler, Control::Cancel),
        };
        placer_ids.insert(placer, 2)?;
        if let Some(filler) = filler {
            filler_ids.insert(filler, 3)?;
        }
        let placer = identity(placer, header.version, 2)?;
        let filler = filler
            .map(|id| identity(id, header.version, 3))
            .transpose()?
            .unwrap_or_default();
        match order {
            Order::Cancel {
                placer: _,
                filler: _,
            } => segment(
                &mut output,
                b"ORC",
                &[control.wire(), &placer, &filler],
                options.limits.message_bytes,
            )?,
            Order::New {
                placer: _,
                filler: _,
                test,
                specimen,
                priority,
                timing,
            } => {
                let mut schedule = b"^^^".to_vec();
                if let Some(start) = timing.start {
                    schedule.extend_from_slice(start.raw());
                }
                schedule.push(b'^');
                if let Some(end) = timing.end {
                    schedule.extend_from_slice(end.raw());
                }
                schedule.push(b'^');
                schedule.extend_from_slice(priority.wire());
                segment(
                    &mut output,
                    b"ORC",
                    &[control.wire(), &placer, &filler, b"", b"", b"F", &schedule],
                    options.limits.message_bytes,
                )?;
                let test = code(
                    *test,
                    b'^',
                    match header.version {
                        Version::V231 => 200,
                        Version::V251 => 250,
                    },
                    4,
                )?;
                let source = code(specimen.source, b'&', 300, 15)?;
                let sample_field = match specimen.field {
                    SampleField::Placer1 => 18,
                    SampleField::Placer2 => 19,
                };
                let sample = escape(specimen.sample.raw(), "OBR", sample_field, 60)?;
                let number = offset.checked_add(1).ok_or(Error::Limit)?.to_string();
                let mut fields = vec![b"".as_slice(); 27];
                for (field, value) in [
                    (1, number.as_bytes()),
                    (2, placer.as_slice()),
                    (3, filler.as_slice()),
                    (4, test.as_slice()),
                    (15, source.as_slice()),
                    (sample_field, sample.as_slice()),
                    (27, schedule.as_slice()),
                ] {
                    let target = field
                        .checked_sub(1)
                        .and_then(|n| fields.get_mut(n))
                        .ok_or(Error::Limit)?;
                    *target = value;
                }
                segment(&mut output, b"OBR", &fields, options.limits.message_bytes)?;
            }
        }
    }
    let message = Message::parse(
        &output,
        Options {
            terminator: Terminator::Cr,
            ..options
        },
    )?;
    crate::Msh::new(&message)?;
    Ok(OrderMessage { message })
}
