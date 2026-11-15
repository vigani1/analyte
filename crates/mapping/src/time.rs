//! Partial source times keep written precision and never acquire an implied zone.
use crate::{Error, Position, text};
use analyte_model::{SourceTime, TimePrecision, UtcOffset};

pub(crate) fn parse(raw: &str, position: Position) -> Result<SourceTime, Error> {
    let mut result = SourceTime {
        original: text(raw, position)?,
        parsed: None,
        offset: None,
        precision: TimePrecision::Unknown,
    };
    let zone_start = raw
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let base = if let Some(start) = zone_start {
        let Some(zone) = raw.get(start..) else {
            return Ok(result);
        };
        let Some(hours) = zone.get(1..3).and_then(|s| s.parse::<i32>().ok()) else {
            return Ok(result);
        };
        let Some(minutes) = zone.get(3..5).and_then(|s| s.parse::<i32>().ok()) else {
            return Ok(result);
        };
        if zone.len() != 5 || hours > 23 || minutes > 59 {
            return Ok(result);
        }
        let Some(seconds) = hours
            .checked_mul(3600)
            .and_then(|h| minutes.checked_mul(60).and_then(|m| h.checked_add(m)))
            .and_then(|s| s.checked_mul(if zone.starts_with('-') { -1 } else { 1 }))
        else {
            return Ok(result);
        };
        result.offset = UtcOffset::try_from(seconds).ok();
        raw.get(..start).unwrap_or_default()
    } else {
        raw
    };
    let (digits, fraction) = base
        .split_once('.')
        .map_or((base, None), |(d, f)| (d, Some(f)));
    if !digits.bytes().all(|b| b.is_ascii_digit())
        || fraction
            .is_some_and(|f| f.is_empty() || f.len() > 4 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return Ok(result);
    }
    let part = |start, end| digits.get(start..end).and_then(|s| s.parse::<i16>().ok());
    let Some(year) = part(0, 4) else {
        return Ok(result);
    };
    if !(1..=9999).contains(&year) {
        return Ok(result);
    }
    let month = part(4, 6).unwrap_or(1);
    let day = part(6, 8).unwrap_or(1);
    let hour = part(8, 10).unwrap_or(0);
    let minute = part(10, 12).unwrap_or(0);
    let second = part(12, 14).unwrap_or(0);
    let precision = match (digits.len(), fraction) {
        (4, None) => TimePrecision::Year,
        (6, None) => TimePrecision::Month,
        (8, None) => TimePrecision::Day,
        (10, None) => TimePrecision::Hour,
        (12, None) => TimePrecision::Minute,
        (14, None) => TimePrecision::Second,
        (14, Some(_)) => TimePrecision::FractionalSecond,
        _ => return Ok(result),
    };
    let Ok(month) = i8::try_from(month) else {
        return Ok(result);
    };
    let Ok(day) = i8::try_from(day) else {
        return Ok(result);
    };
    let Ok(hour) = i8::try_from(hour) else {
        return Ok(result);
    };
    let Ok(minute) = i8::try_from(minute) else {
        return Ok(result);
    };
    let Ok(second) = i8::try_from(second) else {
        return Ok(result);
    };
    let nanos = fraction.map_or(0, |f| format!("{f:0<9}").parse::<i32>().unwrap_or(0));
    let Ok(parsed) = jiff::civil::DateTime::new(year, month, day, hour, minute, second, nanos)
    else {
        return Ok(result);
    };
    result.precision = precision;
    // Validation may fill missing components locally, but the public interpretation
    // must not claim precision the source did not supply.
    if digits.len() == 14 {
        result.parsed = Some(parsed);
    }
    Ok(result)
}
