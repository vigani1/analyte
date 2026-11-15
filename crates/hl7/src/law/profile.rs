//! LAW field and component constraints, from TF-2b Appendix C and section 2.2.
use super::{Error, content, raw};
use crate::{Message, Segment};

/// LAW usage, including the two availability-dependent receiver obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    /// Mandatory: present and valued, including explicit unknown where permitted.
    Mandatory,
    /// Required: sender supports it; null represents an unavailable required value.
    Required,
    /// Required if available at the sending manager.
    RequiredIfAvailable,
    /// Required if available from the analyzer.
    RequiredFromAnalyzer,
    /// Not supported in this context.
    Excluded,
    /// Conditional usage resolved by the owning segment predicate.
    Conditional,
}

/// A field constraint with an already-resolved conditional usage.
#[derive(Debug, Clone, Copy)]
pub struct FieldRule {
    /// Segment owning the rule.
    pub segment: &'static str,
    /// One-based field position.
    pub field: usize,
    /// Effective usage; unresolved Conditional is rejected.
    pub usage: Usage,
    /// Minimum repetitions when the field is present.
    pub minimum: usize,
    /// Maximum repetitions.
    pub maximum: usize,
    /// Maximum encoded bytes per repetition.
    pub length: usize,
}

impl FieldRule {
    /// Checks presence, exclusion, repetition count and length without changing input.
    /// Availability cannot be inferred by a receiver; absent RE/RE.AN remains valid.
    /// # Errors
    /// Returns the owning field for a violated constraint or unresolved predicate.
    pub fn check(self, segment: Segment<'_>) -> Result<(), Error> {
        let value = raw(segment, self.field);
        match self.usage {
            Usage::Mandatory | Usage::Required => {
                if value.is_empty() {
                    return Err(content(self.segment, self.field));
                }
            }
            Usage::Excluded => {
                if !value.is_empty() {
                    return Err(content(self.segment, self.field));
                }
                return Ok(());
            }
            Usage::Conditional => return Err(content(self.segment, self.field)),
            Usage::RequiredIfAvailable | Usage::RequiredFromAnalyzer => {
                if value.is_empty() {
                    return Ok(());
                }
            }
        }
        let field = segment
            .field(self.field)
            .ok_or(content(self.segment, self.field))?;
        let mut count = 0usize;
        for repetition in field.repetitions() {
            if repetition.raw().len() > self.length {
                return Err(content(self.segment, self.field));
            }
            count = count
                .checked_add(1)
                .ok_or(content(self.segment, self.field))?;
        }
        if count < self.minimum || count > self.maximum {
            return Err(content(self.segment, self.field));
        }
        Ok(())
    }
}

fn check(
    segment: Segment<'_>,
    id: &'static str,
    field: usize,
    required: bool,
    length: usize,
    maximum: usize,
) -> Result<(), Error> {
    FieldRule {
        segment: id,
        field,
        usage: if required {
            Usage::Mandatory
        } else {
            Usage::RequiredFromAnalyzer
        },
        minimum: 1,
        maximum,
        length,
    }
    .check(segment)
}

fn coded(
    segment: Segment<'_>,
    id: &'static str,
    field: usize,
    description: bool,
) -> Result<(), Error> {
    let value = raw(segment, field);
    if value.is_empty() || value == b"\"\"" {
        return Ok(());
    }
    let delimiter = segment.delimiters.component;
    for repetition in value.split(|b| *b == segment.delimiters.repetition) {
        let parts: Vec<_> = repetition.split(|b| *b == delimiter).collect();
        for (index, maximum) in [(0, 20), (1, 199), (2, 12)] {
            let component = parts.get(index).copied().unwrap_or_default();
            if component.len() > maximum || ((index != 1 || description) && component.is_empty()) {
                return Err(content(id, field));
            }
        }
    }
    Ok(())
}

/// Checks shared LAW segment constraints without interpreting clinical content.
/// This entry point checks Appendix C fields independently of message grouping.
/// Control segments use the strict send policy; use `validate_control` for
/// directional control validation and cross-segment predicates.
/// # Errors
/// Returns a segment/field location for missing fields, excess repetitions or invalid codes.
pub fn validate_segment(segment: Segment<'_>) -> Result<(), Error> {
    if matches!(segment.id(), b"MSH" | b"MSA" | b"ERR") {
        return super::control::validate_segment(segment, super::ControlDirection::Send);
    }
    let (name, allowed): (&str, &[usize]) = match segment.id() {
        b"NTE" => ("NTE", &[1, 2, 3, 4]),
        b"INV" => ("INV", &[1, 2, 3, 4, 12, 16]),
        b"OBR" => ("OBR", &[2, 3, 4, 11, 16, 17, 49]),
        b"OBX" => (
            "OBX",
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 18, 19, 21, 29],
        ),
        b"ORC" => ("ORC", &[1, 2, 4, 5, 8, 9, 21, 27]),
        b"PID" => ("PID", &[3, 5, 7, 8, 10, 35]),
        b"PV1" => ("PV1", &[2, 3]),
        b"SAC" => ("SAC", &[3, 4, 9, 10, 11, 13, 14, 15, 21, 22, 24, 29]),
        b"SPM" => ("SPM", &[1, 2, 3, 4, 7, 8, 9, 11, 13, 16, 17, 18, 27]),
        b"TCD" => ("TCD", &[1, 2, 3, 5, 6, 7, 8, 9, 10, 11]),
        b"TQ1" => ("TQ1", &[9]),
        b"QPD" | b"RCP" | b"QAK" => return Ok(()),
        _ => return Err(content("LAW", 0)),
    };
    for (offset, field) in segment.fields().enumerate() {
        // MSH iteration includes MSH-1, so one-based positions apply to every segment.
        let number = offset.checked_add(1).ok_or(content(name, 0))?;
        if !allowed.contains(&number) && !field.raw().is_empty() {
            return Err(content(name, number));
        }
    }
    match segment.id() {
        b"NTE" => {
            positive_integer(raw(segment, 1), "NTE", 1)?;
            for (field, length) in [(1, 4), (2, 1), (3, 65536)] {
                check(
                    segment,
                    "NTE",
                    field,
                    true,
                    length,
                    if field == 3 { 1024 } else { 1 },
                )?;
            }
            if !matches!(raw(segment, 2), b"A" | b"Z") {
                return Err(content("NTE", 2));
            }
            coded(segment, "NTE", 4, false)?;
        }
        b"INV" => {
            for field in [1, 2, 3] {
                check(segment, "INV", field, true, 250, 1)?;
                coded(segment, "INV", field, false)?;
            }
        }
        b"OBR" => {
            check(segment, "OBR", 2, true, 50, 1)?;
            check(segment, "OBR", 4, true, 250, 1)?;
            coded(segment, "OBR", 4, true)?;
        }
        b"OBX" => {
            positive_integer(raw(segment, 1), "OBX", 1)?;
            for (field, length, maximum) in [
                (1, 4, 1),
                (3, 250, 1),
                (4, 20, 1),
                (5, 99999, 1024),
                (8, 250, 1024),
                (11, 1, 1),
                (16, 250, 1024),
                (18, 427, 1024),
                (19, 14, 1),
                (29, 4, 1),
            ] {
                check(segment, "OBX", field, true, length, maximum)?;
            }
            coded(segment, "OBX", 3, true)?;
            coded(segment, "OBX", 8, false)?;
            if !timestamp(raw(segment, 19), false) {
                return Err(content("OBX", 19));
            }
            if raw(segment, 2) == b"CE" {
                coded(segment, "OBX", 5, true)?;
            }
            let supplemental = raw(segment, 3)
                .split(|b| *b == segment.delimiters.component)
                .nth(3);
            if segment
                .field(5)
                .is_some_and(|field| field.repetitions().count() > 1)
                && !matches!(supplemental, Some(b"S_RAW" | b"S_OTHER"))
            {
                return Err(content("OBX", 5));
            }
            if !matches!(raw(segment, 11), b"X" | b"P" | b"R" | b"F" | b"C") {
                return Err(content("OBX", 11));
            }
            if !matches!(raw(segment, 29), b"AOE" | b"ASC" | b"RSLT" | b"SCI") {
                return Err(content("OBX", 29));
            }
            let null = raw(segment, 5) == b"\"\"";
            check(segment, "OBX", 2, !null, 2, 1)?;
            if null && !raw(segment, 2).is_empty() {
                return Err(content("OBX", 2));
            }
            if !null
                && !matches!(
                    raw(segment, 2),
                    b"CE" | b"ED" | b"EI" | b"NM" | b"NA" | b"RP" | b"SN" | b"ST" | b"TX"
                )
            {
                return Err(content("OBX", 2));
            }
            let numeric = matches!(raw(segment, 2), b"NM" | b"SN");
            check(segment, "OBX", 6, numeric, 250, 1)?;
            if !numeric && !raw(segment, 6).is_empty() {
                return Err(content("OBX", 6));
            }
            let units = raw(segment, 6);
            if numeric && units != b"\"\"" {
                let parts: Vec<_> = units
                    .split(|b| *b == segment.delimiters.component)
                    .collect();
                if parts.get(1).is_none_or(|p| p.is_empty()) {
                    return Err(content("OBX", 6));
                }
                let identifier = parts.first().copied().unwrap_or_default();
                let system = parts.get(2).copied().unwrap_or_default();
                if (!identifier.is_empty() && system != b"UCUM")
                    || (identifier.is_empty() && !system.is_empty())
                {
                    return Err(content("OBX", 6));
                }
            }
            let sub_id: Vec<_> = raw(segment, 4)
                .split(|b| *b == segment.delimiters.component)
                .collect();
            if sub_id.first().is_none_or(|id| id.is_empty()) || sub_id.len() > 3 {
                return Err(content("OBX", 4));
            }
            for id in sub_id.iter().skip(1) {
                if !id.is_empty()
                    && (!id.iter().all(u8::is_ascii_digit) || id.iter().all(|b| *b == b'0'))
                {
                    return Err(content("OBX", 4));
                }
            }
            for field in [16, 18] {
                for repetition in segment
                    .field(field)
                    .ok_or(content("OBX", field))?
                    .repetitions()
                {
                    let mut parts = repetition.components();
                    if parts.next().is_none_or(|p| p.raw().is_empty()) {
                        return Err(content("OBX", field));
                    }
                    if field == 18 && parts.next().is_none_or(|p| p.raw().is_empty()) {
                        return Err(content("OBX", field));
                    }
                }
            }
            let equipment = segment.field(18).ok_or(content("OBX", 18))?;
            if equipment.repetitions().count() < 2 {
                return Err(content("OBX", 18));
            }
        }
        b"ORC" => {
            check(segment, "ORC", 1, true, 2, 1)?;
        }
        b"PID" => {
            check(segment, "PID", 3, true, 250, 1)?;
            check(segment, "PID", 5, true, 250, 1)?;
            if raw(segment, 5) != b"\"\""
                && raw(segment, 5)
                    .split(|b| *b == segment.delimiters.component)
                    .nth(6)
                    != Some(b"L".as_slice())
            {
                return Err(content("PID", 5));
            }
        }
        b"PV1" => {
            if !matches!(
                raw(segment, 2),
                b"E" | b"I" | b"O" | b"P" | b"R" | b"B" | b"C" | b"N" | b"U"
            ) {
                return Err(content("PV1", 2));
            }
        }
        b"SAC" => {
            if raw(segment, 3).is_empty() && raw(segment, 4).is_empty() {
                return Err(content("SAC", 3));
            }
            for field in [3, 4, 10, 13] {
                check(segment, "SAC", field, false, 80, 1)?;
            }
        }
        b"SPM" => {
            positive_integer(raw(segment, 1), "SPM", 1)?;
            for field in [4, 7, 8, 9, 16, 27] {
                check(
                    segment,
                    "SPM",
                    field,
                    field == 4,
                    250,
                    if matches!(field, 9 | 16) { 1024 } else { 1 },
                )?;
                coded(segment, "SPM", field, false)?;
            }
            coded(segment, "SPM", 11, false)?;
            for field in [17, 18] {
                if !raw(segment, field).is_empty() && !timestamp(raw(segment, field), false) {
                    return Err(content("SPM", field));
                }
            }
            let role = raw(segment, 11)
                .split(|b| *b == segment.delimiters.component)
                .next();
            if matches!(role, Some(b"Q" | b"U")) && raw(segment, 4) != b"\"\"" {
                return Err(content("SPM", 4));
            }
            check(segment, "SPM", 1, true, 4, 1)?;
            let specimen_id = raw(segment, 2);
            if !specimen_id.is_empty() && specimen_id != b"\"\"" {
                let first = specimen_id
                    .split(|b| *b == segment.delimiters.component)
                    .next()
                    .unwrap_or_default();
                let parts: Vec<_> = first
                    .split(|b| *b == segment.delimiters.subcomponent)
                    .collect();
                if parts.first().is_none_or(|p| p.is_empty())
                    || (parts.get(1).is_none_or(|p| p.is_empty())
                        && (parts.get(2).is_none_or(|p| p.is_empty())
                            || parts.get(3).is_none_or(|p| p.is_empty())))
                {
                    return Err(content("SPM", 2));
                }
            }
            check(segment, "SPM", 4, true, 250, 1)?;
            check(segment, "SPM", 11, true, 250, 1)?;
        }
        b"TCD" => {
            check(segment, "TCD", 1, true, 250, 1)?;
            coded(segment, "TCD", 1, true)?;
        }
        b"TQ1" => {
            check(segment, "TQ1", 9, true, 250, 1)?;
            coded(segment, "TQ1", 9, false)?;
        }
        b"QPD" | b"RCP" | b"QAK" => {}
        _ => return Err(content("LAW", 0)),
    }
    Ok(())
}

fn positive_integer(value: &[u8], segment: &'static str, field: usize) -> Result<(), Error> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) || value.iter().all(|b| *b == b'0')
    {
        return Err(content(segment, field));
    }
    Ok(())
}

pub(super) fn header(message: &Message, transaction: &[u8], triggered: bool) -> Result<(), Error> {
    super::validate_control(message, super::ControlDirection::Receive)?;
    let msh = message.segments().next().ok_or(content("MSH", 0))?;
    let first = raw(msh, 21)
        .split(|b| *b == message.delimiters().repetition)
        .next()
        .unwrap_or_default();
    let mut components = first.split(|b| *b == message.delimiters().component);
    if components.next() != Some(transaction) || components.next() != Some(b"IHE".as_slice()) {
        return Err(content("MSH", 21));
    }
    let actual_trigger = matches!(
        raw(msh, 9)
            .split(|b| *b == message.delimiters().component)
            .next(),
        Some(b"QBP" | b"OML" | b"OUL")
    );
    if actual_trigger != triggered {
        return Err(content("MSH", 9));
    }
    Ok(())
}

fn digits(raw: &[u8]) -> Option<u16> {
    raw.iter().try_fold(0u16, |value, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value
            .checked_mul(10)?
            .checked_add(u16::from(byte.checked_sub(b'0')?))
    })
}

pub(super) fn timestamp(value: &[u8], zone: bool) -> bool {
    if value.len() != if zone { 19 } else { 14 } {
        return false;
    }
    let part = |start, end| value.get(start..end).and_then(digits);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        part(0, 4),
        part(4, 6),
        part(6, 8),
        part(8, 10),
        part(10, 12),
        part(12, 14),
    ) else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => return false,
    };
    if year == 0 || day == 0 || day > days || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    if zone {
        if !matches!(value.get(14), Some(b'+' | b'-')) {
            return false;
        }
        if part(15, 17).is_none_or(|v| v > 23) || part(17, 19).is_none_or(|v| v > 59) {
            return false;
        }
    }
    true
}
