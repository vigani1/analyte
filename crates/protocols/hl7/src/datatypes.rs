//! Checks HL7 datatypes to a fixed depth without synthesizing defaults.
use crate::version::{CodingVersions, TimeFormat};
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
        let sign = *value.get(time.len())?;
        if !timezone_offset(sign, zone) {
            return None;
        }
    }
    let mut components = time.split(|b| *b == b'.');
    let whole = components.next()?;
    if !Version::V251
        .rules()?
        .timestamp_precisions
        .contains(&whole.len())
        || !whole.iter().all(u8::is_ascii_digit)
    {
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

pub(super) fn edition_timestamp(parts: &[Component<'_>], version: Version) -> bool {
    let Some(rules) = version.rules() else {
        return false;
    };
    timestamp(parts) && (rules.time_format == TimeFormat::Ts || parts.len() == 1)
}

pub(super) fn coded(parts: &[Component<'_>], version: Version) -> bool {
    parts.iter().copied().all(scalar)
        && parts.iter().any(|p| !p.raw().is_empty())
        && coded_parts(
            &parts.iter().map(|p| p.raw()).collect::<Vec<_>>(),
            version
                .rules()
                .is_some_and(|rules| rules.coded_components >= 9),
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
    let Some(rules) = version.rules() else {
        return false;
    };
    if !simple_parts(
        parts,
        if extended {
            rules.coded_components.max(9)
        } else {
            6
        },
    ) {
        return false;
    }
    if !lengths(parts, rules.coded_lengths) {
        return false;
    }
    // A local CWE code remains opaque, but its non-HL7 coding system needs
    // a supplied version. Never invent a version for the sending application.
    if extended {
        for (code, system, revision, oid) in [(1, 3, 7, 14), (4, 6, 8, 17), (10, 12, 13, 20)]
            .into_iter()
            .take(if rules.coded_components == 22 { 3 } else { 2 })
        {
            if rules.coded_components == 22
                && !at(parts, code).is_empty()
                && at(parts, system).is_empty()
                && at(parts, oid).is_empty()
            {
                return false;
            }
            let system = at(parts, system);
            if !system.is_empty()
                && at(parts, revision).is_empty()
                && (rules.coding_versions == CodingVersions::Explicit
                    || (!at(parts, code).is_empty() && !system.starts_with(b"HL7")))
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
    let Some(rules) = version.rules() else {
        return false;
    };
    (rules.uri || value != b"URI")
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
    let Some(rules) = version.rules() else {
        return false;
    };
    let components: Vec<_> = value.components().collect();
    let limit = rules.xcn_components;
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
                simple_parts(&parts, rules.family_components)
                    && (rules.family_lengths.is_empty()
                        || component.raw().is_empty()
                        || (!at(&parts, 1).is_empty() && lengths(&parts, rules.family_lengths)))
            }
            9 | 14 => hd(&parts, version),
            16 => coded_parts(&parts, rules.coded_components >= 9, version),
            // DR nested in XCN has no sub-subcomponent delimiter; its two
            // date/time values are demoted scalar DTM values (2.A.86.17 erratum).
            17 => {
                (!rules.refused_deprecated.contains(&17) || component.raw().is_empty())
                    && simple_parts(&parts, 2)
                    && parts.iter().all(|v| v.is_empty() || date_time(v).is_some())
            }
            19 | 20 => {
                component.raw().is_empty()
                    || (ts(&parts) && (rules.time_format == TimeFormat::Ts || parts.len() == 1))
            }
            22 | 23 => coded_parts(&parts, true, version),
            _ => scalar(*component),
        };
        if !valid {
            return false;
        }
        if rules
            .xcn_lengths
            .get(offset)
            .is_some_and(|length| component.raw().len() > *length)
        {
            return false;
        }
    }
    let name_types = rules.name_types;
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
    let Some(rules) = version.rules() else {
        return false;
    };
    if !simple_parts(parts, 11) || !matches!(at(parts, 1), b"S" | b"C") {
        return false;
    }
    if rules
        .sequence_required
        .iter()
        .any(|n| at(parts, *n).is_empty())
    {
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
    lengths(parts, rules.sequence_lengths)
}

pub(super) fn timing(value: Repetition<'_>, version: Version) -> bool {
    let Some(rules) = version.rules() else {
        return false;
    };
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
                    && lengths(&parts, rules.interval_lengths)
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
        if rules
            .timing_lengths
            .get(offset)
            .is_some_and(|maximum| component.raw().len() > *maximum)
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // HL7 2.3.1; HL7 2.4 errata of 2002-04-08
    #[test]
    fn order_sequencing_component_model() {
        let valid: &[&[u8]] = &[
            b"S", b"00001", b"LAB", b"00002", b"LAB", b"", b"2", b"1.2.3", b"ISO", b"1.2.4", b"ISO",
        ];
        for version in [Version::V23, Version::V24, Version::V25] {
            assert!(sequence(valid, version));
            let mut invalid = valid.to_vec();
            invalid[6] = b"two";
            assert!(!sequence(&invalid, version));
            let mut extra = valid.to_vec();
            extra.push(b"unsettled");
            assert!(!sequence(&extra, version));
        }
    }

    // HL7 2.3.1 §4.4; HL7 2.5.1 §2.A.50
    #[test]
    fn order_sequence_required_fields_and_lengths() {
        let valid: &[&[u8]] = &[
            b"S", b"placer", b"", b"filler", b"", b"", b"", b"uid1", b"", b"uid2",
        ];
        assert!(sequence(valid, Version::V231));
        assert!(sequence(valid, Version::V251));
        for field in [2_usize, 4, 8, 10] {
            let mut missing = valid.to_vec();
            missing[field - 1] = b"";
            assert!(sequence(&missing, Version::V231));
            assert!(!sequence(&missing, Version::V251));
            let mut longer = valid.to_vec();
            longer[field - 1] = b"1234567890123456";
            assert!(sequence(&longer, Version::V231));
            assert!(!sequence(&longer, Version::V251));
        }
    }

    // HL7 2.3.1 §2.8.3; HL7 2.5.1 §2.A.6
    #[test]
    fn coded_component_length_limits() {
        for (field, maximum) in [(0, 20), (1, 199), (2, 20), (3, 20), (4, 199), (5, 20)] {
            let at_limit = vec![b'A'; maximum];
            let beyond = vec![b'A'; maximum + 1];
            let mut parts = vec![b"".as_slice(); 6];
            parts[field] = &at_limit;
            assert!(coded_parts(&parts, false, Version::V251));
            parts[field] = &beyond;
            assert!(coded_parts(&parts, false, Version::V231));
            assert!(!coded_parts(&parts, false, Version::V251));
        }
    }
}

// HL7 2.5.1 2.A.22: explicit supported offset policy; preserve the wire spelling.
pub(crate) fn timezone_offset(sign: u8, zone: &[u8]) -> bool {
    if zone.len() != 4 || !matches!(sign, b'+' | b'-') {
        return false;
    }
    let Some(hour) = zone.get(..2).and_then(digits) else {
        return false;
    };
    let Some(minute) = zone.get(2..).and_then(digits) else {
        return false;
    };
    let maximum = if sign == b'-' { 12 } else { 14 };
    matches!(minute, 0 | 30 | 45) && (hour < maximum || (hour == maximum && minute == 0))
}

// HL7 2.5.1 2.A.33.2: only ISO OIDs are supported universal identities.
pub(crate) fn universal_id(value: &[u8], kind: &[u8]) -> bool {
    if kind != b"ISO" || value.is_empty() || value.len() > 199 {
        return false;
    }
    let mut arcs = value.split(|b| *b == b'.');
    if !matches!(arcs.next(), Some(b"0" | b"1" | b"2")) {
        return false;
    }
    let mut has_arc = false;
    for arc in arcs {
        if arc.is_empty()
            || !arc.iter().all(u8::is_ascii_digit)
            || (arc.len() > 1 && arc.first() == Some(&b'0'))
        {
            return false;
        }
        has_arc = true;
    }
    has_arc
}

#[cfg(test)]
mod standards_pass_5 {
    use super::*;
    // HL7 2.5.1 §2.A.33.2; IHE PaLM TF-2b
    #[test]
    fn iso_universal_ids_require_oid_syntax() {
        for value in [b"1.2.3".as_slice(), b"0.0", b"2.999999999999999999999"] {
            assert!(universal_id(value, b"ISO"));
        }
        for value in [
            b"word".as_slice(),
            b"1..2",
            b"1.02",
            b"3.1",
            b"1.",
            b".1",
            b"1.a",
            b"1",
        ] {
            assert!(!universal_id(value, b"ISO"));
        }
        assert!(!universal_id(b"1.2.3", b"URI"));
    }
    // HL7 2.5.1 §2.A.22
    #[test]
    fn supported_timezone_offsets() {
        for value in [
            b"20240229235959-1200".as_slice(),
            b"20260101000000+1400",
            b"20260101000000+0545",
            b"20260101000000-0330",
            b"20260101000000-0000",
            b"20260101000000+0000",
        ] {
            assert!(date_time(value).is_some());
        }
        for value in [
            b"20240229235959-2359".as_slice(),
            b"20260101000000+2359",
            b"20260101000000+1401",
            b"20260101000000-1230",
            b"20260101000000+0015",
        ] {
            assert!(date_time(value).is_none());
        }
    }
}
