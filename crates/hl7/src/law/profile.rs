//! LAW field and component constraints, from TF-2b Appendix C and ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â§2.2.
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
        for (index, maximum) in [(0, 20), (1, 199), (2, 20)] {
            let component = parts.get(index).copied().unwrap_or_default();
            if component.len() > maximum || ((index != 1 || description) && component.is_empty()) {
                return Err(content(id, field));
            }
        }
    }
    Ok(())
}

/// Checks shared LAW segment constraints without interpreting clinical content.
/// This entry point checks the Appendix C fields independently of message grouping.
/// # Errors
/// Returns a segment/field location for missing fields, excess repetitions or invalid codes.
pub fn validate_segment(segment: Segment<'_>) -> Result<(), Error> {
    let (name, allowed): (&str, &[usize]) = match segment.id() {
        b"MSH" => ("MSH", &[1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 15, 16, 18, 21]),
        b"MSA" => ("MSA", &[1, 2]),
        b"ERR" => ("ERR", &[2, 3, 4, 5, 8]),
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
        b"MSH" => {
            for (field, length) in [(7, 26), (9, 15), (10, 50), (11, 3), (12, 60), (18, 16)] {
                check(segment, "MSH", field, true, length, 1)?;
            }
            if !timestamp(raw(segment, 7), true) {
                return Err(content("MSH", 7));
            }
            if raw(segment, 12) != b"2.5.1" {
                return Err(content("MSH", 12));
            }
            if raw(segment, 11) != b"P" {
                return Err(content("MSH", 11));
            }
            if raw(segment, 18) != b"UNICODE UTF-8" {
                return Err(content("MSH", 18));
            }
            check(segment, "MSH", 21, true, 427, 1024)?;
        }
        b"MSA" => {
            check(segment, "MSA", 1, true, 2, 1)?;
            check(segment, "MSA", 2, true, 50, 1)?;
            if !matches!(raw(segment, 1), b"AA" | b"AE" | b"AR") {
                return Err(content("MSA", 1));
            }
        }
        b"ERR" => {
            check(segment, "ERR", 2, false, 18, 1024)?;
            check(segment, "ERR", 3, true, 705, 1)?;
            check(segment, "ERR", 4, true, 2, 1)?;
            if !matches!(raw(segment, 4), b"E" | b"W" | b"I") {
                return Err(content("ERR", 4));
            }
        }
        b"NTE" => {
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
                coded(segment, "INV", field, true)?;
            }
        }
        b"OBR" => {
            check(segment, "OBR", 2, true, 50, 1)?;
            check(segment, "OBR", 4, true, 250, 1)?;
            coded(segment, "OBR", 4, true)?;
        }
        b"OBX" => {
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
            if !matches!(
                raw(segment, 11)
                    .split(|b| *b == segment.delimiters.component)
                    .next(),
                Some(b"P" | b"Q" | b"L" | b"U")
            ) {
                return Err(content("SPM", 11));
            }
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

pub(super) fn header(message: &Message, transaction: &[u8], triggered: bool) -> Result<(), Error> {
    let msh = message.segments().next().ok_or(content("MSH", 0))?;
    validate_segment(msh)?;
    let d = message.delimiters();
    let first = raw(msh, 21)
        .split(|b| *b == d.repetition)
        .next()
        .unwrap_or_default();
    if !first.split(|b| *b == d.component).eq([transaction, b"IHE"]) {
        return Err(content("MSH", 21));
    }
    for (field, wanted) in [
        (15, if triggered { b"NE".as_slice() } else { b"" }),
        (16, if triggered { b"AL".as_slice() } else { b"" }),
    ] {
        if raw(msh, field) != wanted {
            return Err(content("MSH", field));
        }
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
