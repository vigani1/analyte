use super::{
    Code, Control, Error, Family, Header, Order, OrderId, OrderMessage, SampleField, Subject,
    invalid, types::scalar,
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
    crate::encoder::append(output, value, maximum).map_err(|_| Error::Limit)
}

fn segment(
    output: &mut Vec<u8>,
    name: &[u8],
    fields: &[&[u8]],
    maximum: usize,
) -> Result<(), Error> {
    crate::encoder::segment(output, name, fields, maximum, b'|').map_err(Into::into)
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

#[derive(Clone, Copy)]
enum CodeForm {
    Ce,
    Cwe,
}

fn code(
    code: Code<'_>,
    separator: u8,
    maximum: usize,
    segment: &str,
    field: usize,
    form: CodeForm,
) -> Result<Vec<u8>, Error> {
    let mut raw = code.identifier.to_vec();
    raw.push(separator);
    raw.extend_from_slice(code.text);
    raw.push(separator);
    raw.extend_from_slice(code.system);
    match form {
        CodeForm::Ce => {
            if code.version.is_some() {
                return Err(invalid(segment, field));
            }
        }
        CodeForm::Cwe => {
            if !code.system.starts_with(b"HL7") && code.version.is_none() {
                return Err(invalid(segment, field));
            }
            if let Some(version) = code.version {
                for _ in 0..4 {
                    raw.push(separator);
                }
                raw.extend_from_slice(version);
            }
        }
    }
    if raw.len() > maximum {
        return Err(invalid(segment, field));
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
    build(orders, subject, header, options, Family::Orm)
}

/// Builds a non-LAW OML/O21 using the existing caller-supplied order types.
///
/// New work uses one SPM per order, sample identity in SPM-2 and the supplied
/// source in SPM-4. `Specimen::field` applies only to ORM; no local OBR sample
/// field is populated here. Timing uses TQ1. A cancellation needs no invented
/// specimen/container. Non-HL7 specimen coding systems require a supplied version.
///
/// # Errors
/// Rejects editions other than 2.5.1, invalid/duplicate work, missing coding versions
/// and output beyond the caller's configured bounds.
pub fn oml(
    orders: &[Order<'_>],
    subject: Subject<'_>,
    header: Header<'_>,
    options: Options,
) -> Result<OrderMessage, Error> {
    if header.version != Version::V251 {
        return Err(invalid("MSH", 12));
    }
    build(orders, subject, header, options, Family::Oml)
}

fn build(
    orders: &[Order<'_>],
    subject: Subject<'_>,
    header: Header<'_>,
    options: Options,
    family: Family,
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
            family.request(),
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
        // The optional patient group cannot be emitted without PID-5.
        // HL7 2.3.1 section 3.3.2; omit it when only an identifier is known.
        Subject::NonPatient | Subject::Unspecified | Subject::Identifier { .. } => {}
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
    let mut specimen_number = 0_usize;
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
                    &[
                        control.wire(),
                        &placer,
                        &filler,
                        b"",
                        b"",
                        b"F",
                        match family {
                            Family::Orm => &schedule,
                            Family::Oml => b"",
                        },
                    ],
                    options.limits.message_bytes,
                )?;
                let test = code(
                    *test,
                    b'^',
                    match header.version {
                        Version::V231 => 200,
                        Version::V251 => 250,
                    },
                    "OBR",
                    4,
                    CodeForm::Ce,
                )?;
                let sample_field = match family {
                    Family::Orm => match specimen.field {
                        SampleField::Placer1 => 18,
                        SampleField::Placer2 => 19,
                    },
                    Family::Oml => 2,
                };
                let (source, sample) = match family {
                    Family::Orm => (
                        specimen
                            .source
                            .map(|source| code(source, b'&', 300, "OBR", 15, CodeForm::Ce))
                            .transpose()?
                            .unwrap_or_default(),
                        escape(specimen.sample.raw(), "OBR", sample_field, 60)?,
                    ),
                    Family::Oml => (
                        code(
                            specimen.source.ok_or_else(|| invalid("SPM", 4))?,
                            b'^',
                            250,
                            "SPM",
                            4,
                            CodeForm::Cwe,
                        )?,
                        escape(
                            specimen.sample.raw(),
                            "SPM",
                            2,
                            super::laboratory::SPECIMEN_ID_BYTES,
                        )?,
                    ),
                };
                if family == Family::Oml {
                    let mut priority_code = priority.wire().to_vec();
                    priority_code.extend_from_slice(b"^^HL70485");
                    segment(
                        &mut output,
                        b"TQ1",
                        &[
                            b"1",
                            b"",
                            b"",
                            b"",
                            b"",
                            b"",
                            timing.start.map_or(b"", |v| v.raw()),
                            timing.end.map_or(b"", |v| v.raw()),
                            &priority_code,
                        ],
                        options.limits.message_bytes,
                    )?;
                }
                let number = match family {
                    Family::Orm => offset.checked_add(1).ok_or(Error::Limit)?,
                    Family::Oml => {
                        specimen_number = specimen_number.checked_add(1).ok_or(Error::Limit)?;
                        specimen_number
                    }
                }
                .to_string();
                if family == Family::Oml && number.len() > 4 {
                    return Err(invalid("SPM", 1));
                }
                let mut fields = vec![b"".as_slice(); 27];
                for (field, value) in [
                    (1_usize, number.as_bytes()),
                    (2, placer.as_slice()),
                    (3, filler.as_slice()),
                    (4, test.as_slice()),
                ] {
                    let target = field
                        .checked_sub(1)
                        .and_then(|n| fields.get_mut(n))
                        .ok_or(Error::Limit)?;
                    *target = value;
                }
                if family == Family::Orm {
                    for (field, value) in [
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
                }
                segment(&mut output, b"OBR", &fields, options.limits.message_bytes)?;
                if family == Family::Oml {
                    segment(
                        &mut output,
                        b"SPM",
                        &[
                            specimen_number.to_string().as_bytes(),
                            &sample,
                            b"",
                            &source,
                        ],
                        options.limits.message_bytes,
                    )?;
                }
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
    Ok(OrderMessage { message, family })
}

/// Adds explicitly supplied placer group and optional position to a built order.
/// Placer-owned OBR-18/19 destinations cannot overwrite existing sample content.
/// # Errors
/// Refuses EI length/injection, conflicting positions and configured output limits.
pub fn context(
    sent: &OrderMessage,
    group: &[u8],
    position: Option<(SampleField, &[u8])>,
    options: Options,
) -> Result<OrderMessage, Error> {
    scalar(group, "ORC", 4, super::EI_MAX_BYTES, true)?;
    let mut output = Vec::new();
    for row in sent.message.segments() {
        if row.id() == b"ORC" || row.id() == b"OBR" {
            let count = row
                .fields()
                .count()
                .max(if row.id() == b"ORC" { 4 } else { 19 });
            let mut fields = (1..=count)
                .map(|n| super::raw(row, n).to_vec())
                .collect::<Vec<_>>();
            if row.id() == b"ORC" {
                // ORC-4 repeats ORC-2's complete assigning authority, not just EI.1.
                // Both editions constrain the entire encoded EI to 22 bytes.
                let placer = super::raw(row, 2);
                let authority = placer
                    .iter()
                    .position(|byte| *byte == b'^')
                    .and_then(|start| placer.get(start..))
                    .ok_or_else(|| invalid("ORC", 4))?;
                if group
                    .len()
                    .checked_add(authority.len())
                    .is_none_or(|n| n > super::EI_MAX_BYTES)
                {
                    return Err(invalid("ORC", 4));
                }
                let mut group_number = group.to_vec();
                group_number.extend_from_slice(authority);
                *fields.get_mut(3).ok_or(Error::Limit)? = group_number;
            }
            if row.id() == b"OBR"
                && let Some((field, value)) = position
            {
                let n = match field {
                    SampleField::Placer1 => 17,
                    SampleField::Placer2 => 18,
                };
                let target = fields.get_mut(n).ok_or(Error::Limit)?;
                if !target.is_empty() {
                    return Err(invalid("OBR", n.saturating_add(1)));
                }
                *target = escape(value, "OBR", n.saturating_add(1), 60)?;
            }
            let refs = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
            segment(&mut output, row.id(), &refs, options.limits.message_bytes)?;
        } else {
            append(&mut output, row.raw(), options.limits.message_bytes)?;
            append(&mut output, b"\r", options.limits.message_bytes)?;
        }
    }
    Ok(OrderMessage {
        message: Message::parse(&output, options)?,
        family: sent.family,
    })
}
