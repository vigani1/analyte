//! Fixed-depth datatype checks. Values are borrowed; no defaults are synthesized.
use crate::{Component, Repetition, Version};

pub(super) fn scalar(value: Component<'_>) -> bool {
    value.subcomponents().count() == 1 && value.raw() != b"\"\""
}

fn at<'a>(parts: &[&'a [u8]], number: usize) -> &'a [u8] {
    number
        .checked_sub(1)
        .and_then(|n| parts.get(n))
        .copied()
        .unwrap_or_default()
}

fn choice(value: &[u8], allowed: &[&[u8]]) -> bool {
    value.is_empty() || allowed.contains(&value)
}

fn digits(value: &[u8]) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    value.iter().try_fold(0_u32, |n, b| {
        if !b.is_ascii_digit() {
            return None;
        }
        n.checked_mul(10)?
            .checked_add(u32::from(b.checked_sub(b'0')?))
    })
}

pub(super) fn decimal(value: &[u8]) -> bool {
    let value = value
        .strip_prefix(b"+")
        .or_else(|| value.strip_prefix(b"-"))
        .unwrap_or(value);
    let mut dot = false;
    let mut digit = false;
    for b in value {
        if *b == b'.' && !dot {
            dot = true;
        } else if b.is_ascii_digit() {
            digit = true;
        } else {
            return false;
        }
    }
    digit
}

fn date_time(value: &[u8]) -> Option<usize> {
    let (time, zone) = if let Some(n) = value.iter().position(|b| matches!(b, b'+' | b'-')) {
        (value.get(..n)?, Some(value.get(n.saturating_add(1)..)?))
    } else {
        (value, None)
    };
    if let Some(zone) = zone {
        if zone.len() != 4 {
            return None;
        }
        let hour = digits(zone.get(..2)?)?;
        let minute = digits(zone.get(2..)?)?;
        // Check HHMM spelling without guessing the sender's jurisdiction or
        // applying today's timezone rules to a historical query timestamp.
        if minute > 59 || hour > 23 {
            return None;
        }
    }
    let mut components = time.split(|b| *b == b'.');
    let whole = components.next()?;
    if !matches!(whole.len(), 4 | 6 | 8 | 10 | 12 | 14) || !whole.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if let Some(fraction) = components.next()
        && (whole.len() != 14
            || fraction.is_empty()
            || fraction.len() > 4
            || !fraction.iter().all(u8::is_ascii_digit))
    {
        return None;
    }
    if components.next().is_some() {
        return None;
    }
    let year = digits(whole.get(..4)?)?;
    if year == 0 {
        return None;
    }
    let month = whole.get(4..6).and_then(digits);
    let leap = year.checked_rem(4) == Some(0)
        && (year.checked_rem(100) != Some(0) || year.checked_rem(400) == Some(0));
    let days = match month {
        Some(2) => {
            if leap {
                29
            } else {
                28
            }
        }
        Some(4 | 6 | 9 | 11) => 30,
        Some(1 | 3 | 5 | 7 | 8 | 10 | 12) | None => 31,
        Some(_) => return None,
    };
    for (start, end, min, max) in [
        (6, 8, 1, days),
        (8, 10, 0, 23),
        (10, 12, 0, 59),
        (12, 14, 0, 59),
    ] {
        if let Some(part) = whole.get(start..end)
            && digits(part).is_none_or(|v| v < min || v > max)
        {
            return None;
        }
    }
    Some(whole.len())
}

pub(crate) fn ts(parts: &[&[u8]]) -> bool {
    if parts.is_empty() || parts.len() > 2 || at(parts, 1).len() > 24 {
        return false;
    }
    let Some(precision) = date_time(at(parts, 1)) else {
        return false;
    };
    let required = match at(parts, 2) {
        b"" => 0,
        b"Y" => 4,
        b"L" => 6,
        b"D" => 8,
        b"H" => 10,
        b"M" => 12,
        b"S" => 14,
        _ => return false,
    };
    required <= precision
}

pub(super) fn timestamp(parts: &[Component<'_>]) -> bool {
    parts.iter().copied().all(scalar) && ts(&parts.iter().map(|v| v.raw()).collect::<Vec<_>>())
}

pub(super) fn coded(parts: &[Component<'_>], version: Version) -> bool {
    parts.iter().copied().all(scalar)
        && parts.iter().any(|p| !p.raw().is_empty())
        && coded_parts(
            &parts.iter().map(|p| p.raw()).collect::<Vec<_>>(),
            false,
            version,
        )
}

fn lengths(parts: &[&[u8]], maxima: &[usize]) -> bool {
    parts
        .iter()
        .zip(maxima)
        .all(|(value, max)| value.len() <= *max)
}

pub(crate) fn cwe(parts: &[Component<'_>]) -> bool {
    parts.iter().copied().all(scalar)
        && parts.iter().any(|p| !p.raw().is_empty())
        && coded_parts(
            &parts.iter().map(|p| p.raw()).collect::<Vec<_>>(),
            true,
            Version::V251,
        )
}

fn coded_parts(parts: &[&[u8]], extended: bool, version: Version) -> bool {
    if !simple_parts(parts, if extended { 9 } else { 6 }) {
        return false;
    }
    if version == Version::V251 && !lengths(parts, &[20, 199, 20, 20, 199, 20, 10, 10, 199]) {
        return false;
    }
    // A local CWE code remains opaque, but its non-HL7 coding system needs
    // a supplied version. Never invent a version for the sending application.
    if extended {
        for (code, system, revision) in [(1, 3, 7), (4, 6, 8)] {
            let system = at(parts, system);
            if !at(parts, code).is_empty()
                && !system.is_empty()
                && !system.starts_with(b"HL7")
                && at(parts, revision).is_empty()
            {
                return false;
            }
        }
    }
    true
}

fn simple_parts(parts: &[&[u8]], maximum: usize) -> bool {
    parts.len() <= maximum && parts.iter().all(|v| *v != b"\"\"")
}

pub(crate) fn uid(value: &[u8], version: Version) -> bool {
    (version != Version::V231 || value != b"URI")
        && choice(
            value,
            &[
                b"DNS", b"GUID", b"HCD", b"HL7", b"ISO", b"L", b"M", b"N", b"Random", b"UUID",
                b"URI", b"x400", b"x500",
            ],
        )
}

fn hd(parts: &[&[u8]], version: Version) -> bool {
    simple_parts(parts, 3)
        && at(parts, 1).len() <= 20
        && at(parts, 2).len() <= 199
        && at(parts, 3).len() <= 6
        && at(parts, 2).is_empty() == at(parts, 3).is_empty()
        && uid(at(parts, 3), version)
}

pub(super) fn quantity(value: Repetition<'_>) -> bool {
    let parts: Vec<_> = value.components().collect();
    if parts.len() > 2 {
        return false;
    }
    let Some(amount) = parts.first().copied() else {
        return false;
    };
    if !scalar(amount)
        || (!amount.raw().is_empty() && !decimal(amount.raw()))
        || amount.raw().is_empty()
    {
        return false;
    }
    if let Some(unit) = parts.get(1) {
        let units: Vec<_> = unit.subcomponents().collect();
        if !simple_parts(&units, 6) {
            return false;
        }
        if !choice(at(&units, 1), &[b"CH", b"LI", b"PG", b"RD", b"ZO"]) {
            return false;
        }
    }
    true
}

pub(super) fn xcn(value: Repetition<'_>, version: Version) -> bool {
    let components: Vec<_> = value.components().collect();
    let limit = match version {
        Version::V231 => 15,
        Version::V251 => 23,
    };
    if components.len() > limit {
        return false;
    }
    let raw: Vec<_> = components.iter().map(|v| v.raw()).collect();
    if raw.iter().all(|v| v.is_empty()) {
        return false;
    }
    if !at(&raw, 1).is_empty() && at(&raw, 8).is_empty() && at(&raw, 9).is_empty() {
        return false;
    }
    for (offset, component) in components.iter().enumerate() {
        let n = offset.saturating_add(1);
        let parts: Vec<_> = component.subcomponents().collect();
        if parts.iter().any(|p| *p == b"\"\"") {
            return false;
        }
        let valid = match n {
            2 => {
                simple_parts(
                    &parts,
                    match version {
                        Version::V231 => 2,
                        Version::V251 => 5,
                    },
                ) && (version != Version::V251
                    || component.raw().is_empty()
                    || (!at(&parts, 1).is_empty() && lengths(&parts, &[50, 20, 50, 20, 50])))
            }
            9 | 14 => hd(&parts, version),
            16 => coded_parts(&parts, false, version),
            // DR nested in XCN has no sub-subcomponent delimiter; its two
            // date/time values are demoted scalar DTM values (2.A.86.17 erratum).
            17 => {
                simple_parts(&parts, 2)
                    && parts.iter().all(|v| v.is_empty() || date_time(v).is_some())
            }
            19 | 20 => component.raw().is_empty() || ts(&parts),
            22 | 23 => coded_parts(&parts, true, version),
            _ => scalar(*component),
        };
        if !valid {
            return false;
        }
        if version == Version::V251 {
            let length = match n {
                1 => 15,
                2 => 194,
                3 | 4 => 30,
                5 | 6 => 20,
                7 | 13 => 5,
                8 => 4,
                9 | 14 => 227,
                10 | 11 | 15 | 18 => 1,
                12 => 3,
                16 => 483,
                17 => 53,
                19 | 20 => 26,
                21 => 199,
                22 | 23 => 705,
                _ => return false,
            };
            if component.raw().len() > length {
                return false;
            }
        }
    }
    let name_types: &[&[u8]] = match version {
        Version::V231 => &[b"A", b"D", b"L", b"M", b"C", b"B", b"P", b"S", b"T", b"U"],
        Version::V251 => &[
            b"A", b"B", b"C", b"D", b"I", b"L", b"M", b"N", b"P", b"R", b"S", b"T", b"U",
        ],
    };
    choice(at(&raw, 10), name_types)
        && choice(at(&raw, 12), &[b"M10", b"M11", b"ISO", b"NPI"])
        && (at(&raw, 11).is_empty() || !at(&raw, 12).is_empty())
        && choice(at(&raw, 15), &[b"A", b"I", b"P"])
        && choice(at(&raw, 18), &[b"G", b"F"])
}

fn duration(value: &[u8]) -> bool {
    value.is_empty()
        || value == b"INDEF"
        || (matches!(
            value.first(),
            Some(b'S' | b'M' | b'H' | b'D' | b'W' | b'L' | b'X' | b'T')
        ) && value
            .get(1..)
            .is_some_and(|v| !v.is_empty() && v.iter().all(u8::is_ascii_digit)))
}

fn explicit_times(value: &[u8]) -> bool {
    value.is_empty()
        || value.split(|b| *b == b',').all(|time| {
            time.len() == 4
                && time.get(..2).and_then(digits).is_some_and(|v| v <= 23)
                && time.get(2..).and_then(digits).is_some_and(|v| v <= 59)
        })
}

fn sequence(parts: &[&[u8]], version: Version) -> bool {
    if !simple_parts(parts, 11) || !matches!(at(parts, 1), b"S" | b"C") {
        return false;
    }
    if version == Version::V251 && [1, 2, 4, 8, 10].iter().any(|n| at(parts, *n).is_empty()) {
        return false;
    }
    if !at(parts, 7).is_empty() && !decimal(at(parts, 7)) {
        return false;
    }
    if !uid(at(parts, 9), version) || !uid(at(parts, 11), version) {
        return false;
    }
    let condition = at(parts, 6);
    if !condition.is_empty() {
        if !matches!(condition.get(..2), Some(b"SS" | b"EE" | b"SE" | b"ES")) {
            return false;
        }
        let rest = condition.get(2..).unwrap_or_default();
        if !matches!(rest.first(), Some(b'+' | b'-'))
            || !matches!(rest.get(1), Some(b'S' | b'M' | b'H' | b'D' | b'W' | b'L'))
            || rest
                .get(2..)
                .is_none_or(|v| v.is_empty() || !v.iter().all(u8::is_ascii_digit))
            || rest.len() < 3
        {
            return false;
        }
    }
    version != Version::V251 || lengths(parts, &[1, 15, 6, 15, 6, 12, 3, 15, 6, 15, 6])
}

pub(super) fn timing(value: Repetition<'_>, version: Version) -> bool {
    let components: Vec<_> = value.components().collect();
    if components.len() > 12 {
        return false;
    }
    for (offset, component) in components.iter().enumerate() {
        if component.raw().is_empty() {
            continue;
        }
        let n = offset.saturating_add(1);
        let parts: Vec<_> = component.subcomponents().collect();
        if parts.iter().any(|p| *p == b"\"\"") {
            return false;
        }
        let valid = match n {
            1 => {
                simple_parts(&parts, 2)
                    && at(&parts, 1).len() <= 16
                    && (at(&parts, 1).is_empty() || decimal(at(&parts, 1)))
            }
            2 => {
                simple_parts(&parts, 2)
                    && explicit_times(at(&parts, 2))
                    && (version != Version::V251 || lengths(&parts, &[6, 199]))
            }
            3 => scalar(*component) && duration(component.raw()),
            4 | 5 => ts(&parts),
            6..=8 => scalar(*component),
            9 => scalar(*component) && choice(component.raw(), &[b"S", b"A", b"C"]),
            10 => sequence(&parts, version),
            11 => coded_parts(&parts, false, version),
            12 => scalar(*component) && decimal(component.raw()),
            _ => false,
        };
        if !valid {
            return false;
        }
        if version == Version::V251 {
            let maximum = match n {
                1 => 267,
                2 => 206,
                3 | 6 => 6,
                4 | 5 => 26,
                7 => 199,
                8 => 200,
                9 => 1,
                10 => 110,
                11 => 483,
                12 => 4,
                _ => return false,
            };
            if component.raw().len() > maximum {
                return false;
            }
        }
    }
    true
}
