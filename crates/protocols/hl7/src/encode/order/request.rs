//! Encodes new-order timing, specimens and OBR segments within a bounded buffer.
use super::{
    BuildContext, Code, CodeForm, Control, Error, Family, SampleField, Version, code, escape,
    invalid, scalar, segment,
};
use crate::order;
#[derive(Clone, Copy)]
pub(super) struct NewRequest<'a> {
    pub(super) test: Code<'a>,
    pub(super) specimen: order::Specimen<'a>,
    pub(super) priority: order::Priority,
    pub(super) timing: order::Timing<'a>,
}
#[derive(Clone, Copy)]
struct DetailFields<'a> {
    number: &'a str,
    placer: &'a [u8],
    filler: &'a [u8],
    test: &'a [u8],
    source: &'a [u8],
    sample: &'a [u8],
    schedule: &'a [u8],
    sample_field: usize,
}

pub(super) fn encode_new_order(
    output: &mut Vec<u8>,
    context: BuildContext<'_>,
    request: NewRequest<'_>,
    placer: &[u8],
    filler: &[u8],
    offset: usize,
    specimen_number: &mut usize,
) -> Result<(), Error> {
    let BuildContext {
        header,
        options: _,
        family,
    } = context;
    let NewRequest {
        test,
        specimen,
        priority,
        timing,
    } = request;
    let rules = header
        .version
        .order_rules()
        .ok_or_else(|| invalid("MSH", 12))?;
    if rules.test_cwe
        && (test.version.is_none()
            || specimen
                .source
                .is_some_and(|source| source.version.is_none()))
    {
        return Err(invalid("CWE", 7));
    }
    let schedule = encode_schedule(output, context, priority, timing, placer, filler)?;
    let test = encode_test(test, header.version)?;
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
                order::laboratory::SPECIMEN_ID_BYTES,
            )?,
        ),
    };
    encode_timing(output, context, priority, timing)?;
    let number = match family {
        Family::Orm => offset.checked_add(1).ok_or(Error::Limit)?,
        Family::Oml => {
            *specimen_number = specimen_number.checked_add(1).ok_or(Error::Limit)?;
            *specimen_number
        }
    }
    .to_string();
    if family == Family::Oml && number.len() > 4 {
        return Err(invalid("SPM", 1));
    }
    encode_detail(
        output,
        context,
        DetailFields {
            number: &number,
            placer,
            filler,
            test: &test,
            source: &source,
            sample: &sample,
            schedule: &schedule,
            sample_field,
        },
        *specimen_number,
    )?;
    Ok(())
}

fn encode_detail(
    output: &mut Vec<u8>,
    context: BuildContext<'_>,
    fields: DetailFields<'_>,
    specimen_number: usize,
) -> Result<(), Error> {
    let BuildContext {
        options, family, ..
    } = context;
    let mut row = vec![b"".as_slice(); 27];
    for (field, value) in [
        (1_usize, fields.number.as_bytes()),
        (2, fields.placer),
        (3, fields.filler),
        (4, fields.test),
    ] {
        let target = field
            .checked_sub(1)
            .and_then(|n| row.get_mut(n))
            .ok_or(Error::Limit)?;
        *target = value;
    }
    if family == Family::Orm {
        for (field, value) in [
            (15, fields.source),
            (fields.sample_field, fields.sample),
            (27, fields.schedule),
        ] {
            let target = field
                .checked_sub(1)
                .and_then(|n| row.get_mut(n))
                .ok_or(Error::Limit)?;
            *target = value;
        }
    }
    segment(output, b"OBR", &row, options.limits.message_bytes)?;
    if family == Family::Oml {
        segment(
            output,
            b"SPM",
            &[
                specimen_number.to_string().as_bytes(),
                fields.sample,
                b"",
                fields.source,
            ],
            options.limits.message_bytes,
        )?;
    }
    Ok(())
}

fn encode_schedule(
    output: &mut Vec<u8>,
    context: BuildContext<'_>,
    priority: order::Priority,
    timing: order::Timing<'_>,
    placer: &[u8],
    filler: &[u8],
) -> Result<Vec<u8>, Error> {
    let BuildContext {
        options, family, ..
    } = context;
    let control = Control::New;
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
        output,
        b"ORC",
        &[
            control.wire(),
            placer,
            filler,
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
    Ok(schedule)
}

fn encode_timing(
    output: &mut Vec<u8>,
    context: BuildContext<'_>,
    priority: order::Priority,
    timing: order::Timing<'_>,
) -> Result<(), Error> {
    let BuildContext {
        options,
        family,
        header,
    } = context;
    if family == Family::Oml {
        let mut priority_code = priority.wire().to_vec();
        priority_code.extend_from_slice(b"^^HL70485");
        let rules = header
            .version
            .order_rules()
            .ok_or_else(|| invalid("MSH", 12))?;
        if rules.test_cwe && timing.priority_version.is_none() {
            return Err(invalid("TQ1", 9));
        }
        if let Some(revision) = timing.priority_version {
            scalar(revision, "TQ1", 9, 10, true)?;
            priority_code.extend_from_slice(b"^^^^");
            priority_code.extend_from_slice(revision);
        }
        for (number, time) in [(7, timing.start), (8, timing.end)] {
            if time.is_some_and(|time| time.raw().len() > rules.time_bytes) {
                return Err(invalid("TQ1", number));
            }
        }
        segment(
            output,
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
    Ok(())
}

fn encode_test(test: Code<'_>, version: Version) -> Result<Vec<u8>, Error> {
    let rules = version.order_rules().ok_or_else(|| invalid("MSH", 12))?;
    code(
        test,
        b'^',
        rules.code_bytes,
        "OBR",
        4,
        if rules.test_cwe {
            CodeForm::Cwe
        } else {
            CodeForm::Ce
        },
    )
}
