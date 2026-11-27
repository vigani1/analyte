//! Parses the optional site time zone: an IANA name or a fixed UTC offset.
use serde::{Deserialize, Serialize};

/// The lab's own time zone, used only where a standard lets a sender omit one
/// and the owner chose to tolerate it (IHE LAW MSH-7). It is never applied to a
/// stored value; it only names how a zoneless time is read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SiteTimeZone {
    spelling: String,
    zone: jiff::tz::TimeZone,
}

impl SiteTimeZone {
    /// The setting exactly as written in the site file.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.spelling
    }

    /// The offset that a local date and time has in this zone, or `None` when a
    /// daylight-saving change makes it skipped or repeated: such a time is never
    /// guessed onto one side of the change.
    #[must_use]
    pub fn offset_at(&self, local: jiff::civil::DateTime) -> Option<jiff::tz::Offset> {
        match self.zone.to_ambiguous_timestamp(local).offset() {
            jiff::tz::AmbiguousOffset::Unambiguous { offset } => Some(offset),
            jiff::tz::AmbiguousOffset::Gap { .. } | jiff::tz::AmbiguousOffset::Fold { .. } => None,
        }
    }
}

impl TryFrom<String> for SiteTimeZone {
    type Error = &'static str;
    fn try_from(spelling: String) -> Result<Self, Self::Error> {
        let zone = if spelling.starts_with(['+', '-']) {
            jiff::tz::TimeZone::fixed(fixed_offset(&spelling).ok_or(REFUSAL)?)
        } else if spelling.contains(|c: char| c.is_ascii_control() || c.is_whitespace()) {
            return Err(REFUSAL);
        } else {
            jiff::tz::TimeZone::get(&spelling).map_err(|_| REFUSAL)?
        };
        Ok(Self { spelling, zone })
    }
}

impl From<SiteTimeZone> for String {
    fn from(value: SiteTimeZone) -> Self {
        value.spelling
    }
}

const REFUSAL: &str =
    "time_zone must be an IANA name such as Europe/Berlin or a fixed offset such as +01:00";

/// Reads `+HH:MM` or `-HH:MM` with hours 00 to 23 and minutes 00 to 59.
fn fixed_offset(spelling: &str) -> Option<jiff::tz::Offset> {
    let bytes = spelling.as_bytes();
    let [sign, h1, h2, b':', m1, m2] = bytes else {
        return None;
    };
    let digit = |b: &u8| {
        b.is_ascii_digit()
            .then(|| i32::from(b.saturating_sub(b'0')))
    };
    let hours = digit(h1)?.checked_mul(10)?.checked_add(digit(h2)?)?;
    let minutes = digit(m1)?.checked_mul(10)?.checked_add(digit(m2)?)?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let seconds = hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?;
    let signed = if *sign == b'-' {
        seconds.checked_neg()?
    } else {
        seconds
    };
    jiff::tz::Offset::from_seconds(signed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decision 2026-11-18: one validated site time zone (IANA or fixed offset).
    // IHE PaLM TF-2b §C.4
    #[test]
    fn iana_or_fixed_offset_only() -> Result<(), Box<dyn std::error::Error>> {
        for good in [
            "Europe/Berlin",
            "UTC",
            "America/New_York",
            "+01:00",
            "-05:30",
            "+00:00",
        ] {
            let zone = SiteTimeZone::try_from(good.to_owned())?;
            assert_eq!(zone.as_str(), good);
        }
        for bad in [
            "",
            "Mars/Olympus",
            "+1:00",
            "+01",
            "+0100",
            "+24:00",
            "+01:60",
            "Europe/ Berlin",
            "Europe/Berlin\n",
        ] {
            assert!(SiteTimeZone::try_from(bad.to_owned()).is_err(), "{bad:?}");
        }
        Ok(())
    }

    // A skipped or repeated local hour has no single offset and is never guessed.
    // IHE PaLM TF-2b §C.4
    #[test]
    fn daylight_saving_changes_give_no_offset() -> Result<(), Box<dyn std::error::Error>> {
        let berlin = SiteTimeZone::try_from("Europe/Berlin".to_owned())?;
        let winter = jiff::civil::date(2026, 11, 15).at(13, 34, 37, 0);
        assert_eq!(
            berlin.offset_at(winter),
            Some(jiff::tz::Offset::from_hours(1)?)
        );
        let skipped = jiff::civil::date(2026, 3, 29).at(2, 30, 0, 0);
        let repeated = jiff::civil::date(2026, 10, 25).at(2, 30, 0, 0);
        assert_eq!(berlin.offset_at(skipped), None);
        assert_eq!(berlin.offset_at(repeated), None);
        let fixed = SiteTimeZone::try_from("-05:30".to_owned())?;
        assert_eq!(
            fixed.offset_at(skipped),
            Some(jiff::tz::Offset::from_seconds(-19_800)?)
        );
        Ok(())
    }
}
