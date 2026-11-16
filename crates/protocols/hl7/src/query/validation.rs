//! Validates QRD and QRF fields (HL7 2.3.1 2.24.4–5 and 2.5.1 5.10.5.3–4).
use super::invalid;
use crate::{Component, Error, Repetition, Segment, Version};

use crate::datatypes::{coded, decimal, edition_timestamp, quantity, scalar, timing, xcn};

fn check(
    segment: Segment<'_>,
    name: &'static str,
    number: usize,
    required: bool,
    repeats: bool,
    maximum: usize,
    valid: impl Fn(Repetition<'_>) -> bool,
) -> Result<(), Error> {
    let Some(field) = segment.field(number) else {
        return if required {
            Err(invalid(name, number))
        } else {
            Ok(())
        };
    };
    if field.raw().is_empty() {
        return if required {
            Err(invalid(name, number))
        } else {
            Ok(())
        };
    }
    if field.is_null() || (!repeats && field.repetitions().count() != 1) {
        return Err(invalid(name, number));
    }
    for value in field.repetitions() {
        if value.raw().is_empty()
            || value.raw() == b"\"\""
            || value.raw().len() > maximum
            || !valid(value)
        {
            return Err(invalid(name, number));
        }
    }
    Ok(())
}

fn one(value: Repetition<'_>) -> Option<Component<'_>> {
    let mut parts = value.components();
    let result = parts.next()?;
    parts.next().is_none().then_some(result)
}

fn text(value: Repetition<'_>) -> bool {
    one(value).is_some_and(scalar)
}

fn table(value: Repetition<'_>, allowed: &[&[u8]]) -> bool {
    text(value) && allowed.contains(&value.raw())
}

fn last_field(segment: Segment<'_>, name: &'static str, last: usize) -> Result<(), Error> {
    for (offset, field) in segment.fields().enumerate().skip(last) {
        if !field.raw().is_empty() {
            return Err(invalid(name, offset.saturating_add(1)));
        }
    }
    Ok(())
}

pub(super) fn definition(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    let name = "QRD";
    last_field(segment, name, 12)?;
    check(segment, name, 1, true, false, 26, |v| {
        edition_timestamp(&v.components().collect::<Vec<_>>(), version)
    })?;
    check(segment, name, 2, true, false, 1, |v| {
        table(v, &[b"D", b"R", b"T"])
    })?;
    check(segment, name, 3, true, false, 1, |v| {
        table(v, &[b"D", b"I"])
    })?;
    check(segment, name, 4, true, false, 10, text)?;
    check(segment, name, 5, false, false, 1, |v| {
        table(v, &[b"B", b"L"])
    })?;
    check(segment, name, 6, false, false, 26, |v| {
        edition_timestamp(&v.components().collect::<Vec<_>>(), version)
    })?;
    check(segment, name, 7, true, false, 10, quantity)?;
    let length = version.query_person_bytes()?;
    check(segment, name, 8, true, true, length, |v| xcn(v, version))?;
    for n in [9, 10] {
        check(segment, name, n, true, true, length, |v| {
            coded(&v.components().collect::<Vec<_>>(), version)
        })?;
    }
    check(segment, name, 11, false, true, 20, |v| {
        let parts: Vec<_> = v.components().collect();
        parts.len() <= 2 && parts.into_iter().all(scalar)
    })?;
    check(segment, name, 12, false, false, 1, |v| {
        table(v, &[b"O", b"R", b"S", b"T"])
    })?;
    Ok(())
}

pub(super) fn filter(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    let name = "QRF";
    let rules = version.require_rules()?;
    last_field(segment, name, version.query_filter_fields()?)?;
    check(segment, name, 1, true, true, 20, text)?;
    for n in [2, 3] {
        check(segment, name, n, false, false, 26, |v| {
            edition_timestamp(&v.components().collect::<Vec<_>>(), version)
        })?;
    }
    for n in [4, 5] {
        check(segment, name, n, false, true, 60, text)?;
    }
    check(segment, name, 6, false, true, 12, |v| {
        table(v, &[b"ANY", b"COL", b"ORD", b"RCT", b"REP", b"SCHED"])
    })?;
    check(segment, name, 7, false, true, 12, |v| {
        table(v, &[b"ANY", b"CFN", b"COR", b"FIN", b"PRE", b"REP"])
    })?;
    check(segment, name, 8, false, true, 12, |v| {
        table(v, &[b"1ST", b"ALL", b"LST", b"REV"])
    })?;
    // HL7 2.6 section 5.10.4.2.9: deprecated TQ is outside this bounded profile.
    check(
        segment,
        name,
        9,
        false,
        false,
        rules.query_timing_bytes.unwrap_or(0),
        |v| rules.query_timing_bytes.is_some() && timing(v, version),
    )?;
    if version.query_filter_fields()? == 10 {
        check(segment, name, 10, false, false, 10, |v| {
            text(v) && decimal(v.raw())
        })?;
    }
    // Date precision and offsets are retained. The host decides how to compare
    // date ranges under its agreed timezone; no missing precision is invented.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl7_231_2_24_4_2_3_251_5_10_5_3_3_4_tables_are_distinct_from_transaction_scope()
    -> Result<(), Error> {
        for (edition, version) in [("2.3.1", Version::V231), ("2.5.1", Version::V251)] {
            for format in ["D", "R", "T"] {
                for priority in ["D", "I"] {
                    let bytes = format!(
                        "MSH|^~\\&|A|B|C|D|20261115||QRY^Q02^QRY_Q02|m|P|{edition}\rQRD|20261115|{format}|{priority}|tag|||5^LI|^Synthetic|LOCAL|LOCAL\r"
                    );
                    let message =
                        crate::Message::parse(bytes.as_bytes(), crate::Options::default())?;
                    let segment = message.segments().nth(1).ok_or(Error::Encoding)?;
                    definition(segment, version)?;
                    let supported = super::super::Request::new(&message).is_ok();
                    assert_eq!(supported, format == "D" && priority == "D");
                }
            }
        }
        Ok(())
    }
}
