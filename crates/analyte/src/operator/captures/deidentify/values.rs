//! Makes synthetic stand-ins of the same shape for patient values, one per real value in a file.
//!
//! A stand-in keeps the byte length, the character class of every byte
//! (digit, upper or lower case letter, non-ASCII letter of the same encoded
//! width) and every other byte, such as spaces and punctuation. Leading zeros
//! stay zeros and the first significant digit stays non-zero, so the count of
//! leading zeros is kept. Characters come from the system random source, so a
//! stand-in says nothing about the value it replaces.
use super::Error;
use std::collections::{HashMap, HashSet};

/// How a value is replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Kind {
    /// Same-shape random characters.
    Text,
    /// A valid synthetic date of the same layout when the layout is recognized.
    BirthDate,
    /// A one-letter sex code becomes `U` (unknown); anything longer is text.
    Sex,
}

/// Attempts before a stand-in may repeat another real value of a tiny shape.
const ATTEMPTS: usize = 64;

/// Bytes drawn from the system random source at a time.
const POOL: usize = 256;

/// The system random source, read in bounded blocks.
struct Random {
    pool: [u8; POOL],
    next: usize,
}

impl Random {
    fn new() -> Self {
        Self {
            pool: [0; POOL],
            next: POOL,
        }
    }

    fn byte(&mut self) -> Result<u8, Error> {
        if self.next >= POOL {
            getrandom::fill(&mut self.pool).map_err(Error::Random)?;
            self.next = 0;
        }
        let byte = self.pool.get(self.next).copied().ok_or(Error::Limit)?;
        self.next = self.next.saturating_add(1);
        Ok(byte)
    }

    /// A uniform value in `0..n` by rejection sampling; `n` is 1..=128.
    fn below(&mut self, n: u8) -> Result<u8, Error> {
        let n = n.max(1);
        // The largest multiple of n not above 256 keeps the draw uniform.
        let zone = u16::from(n).saturating_mul(256_u16.checked_div(u16::from(n)).unwrap_or(1));
        loop {
            let byte = self.byte()?;
            if u16::from(byte) < zone {
                return byte.checked_rem(n).ok_or(Error::Limit);
            }
        }
    }

    fn pick(&mut self, from: u8, count: u8) -> Result<u8, Error> {
        Ok(from.saturating_add(self.below(count)?))
    }
}

/// The stand-ins chosen for one file.
pub(super) struct Pseudonyms {
    assigned: HashMap<(Kind, Vec<u8>), Vec<u8>>,
    /// Real values and stand-ins already used; a new text stand-in avoids them.
    taken: HashSet<Vec<u8>>,
    random: Random,
}

impl Pseudonyms {
    /// Starts with every real value in the file, so no stand-in spells another real value.
    pub(super) fn new(reals: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            assigned: HashMap::new(),
            taken: reals.into_iter().collect(),
            random: Random::new(),
        }
    }

    /// Distinct real values replaced so far.
    pub(super) fn distinct(&self) -> usize {
        self.assigned.len()
    }

    /// The stand-in for `real`; the same real value and kind always get the same one.
    /// Bytes marked in `keep` (escape sequences) stay as they are.
    pub(super) fn stand_in(
        &mut self,
        kind: Kind,
        real: &[u8],
        keep: &[bool],
    ) -> Result<Vec<u8>, Error> {
        if let Some(fake) = self.assigned.get(&(kind, real.to_vec())) {
            return Ok(fake.clone());
        }
        let fake = match kind {
            Kind::Sex if real.len() == 1 && real.iter().all(u8::is_ascii_alphabetic) => {
                b"U".to_vec()
            }
            Kind::BirthDate => match self.date(real, keep)? {
                Some(fake) => fake,
                None => self.text(real, keep)?,
            },
            Kind::Sex | Kind::Text => self.text(real, keep)?,
        };
        self.taken.insert(fake.clone());
        self.assigned.insert((kind, real.to_vec()), fake.clone());
        Ok(fake)
    }

    /// Same-shape random text that differs from `real` and, when possible,
    /// from every other real value and stand-in in the file.
    fn text(&mut self, real: &[u8], keep: &[bool]) -> Result<Vec<u8>, Error> {
        let mut fallback = None;
        for _ in 0..ATTEMPTS {
            let fake = shaped(&mut self.random, real, keep)?;
            if fake == real && changeable(real, keep) {
                continue;
            }
            if !self.taken.contains(&fake) || !changeable(real, keep) {
                return Ok(fake);
            }
            fallback.get_or_insert(fake);
        }
        // A tiny shape (one digit, say) may have fewer stand-ins than values.
        fallback.ok_or(Error::Limit)
    }

    /// A valid synthetic date in the same layout, or `None` for an unknown layout.
    fn date(&mut self, real: &[u8], keep: &[bool]) -> Result<Option<Vec<u8>>, Error> {
        if keep.iter().any(|kept| *kept) {
            return Ok(None);
        }
        let groups = digit_groups(real);
        let widths: Vec<usize> = groups.iter().map(std::ops::Range::len).collect();
        let digits_only = groups.len() == 1 && widths.first() == Some(&real.len());
        for _ in 0..ATTEMPTS {
            let mut fake = real.to_vec();
            if digits_only && real.len() >= 8 {
                // YYYYMMDD, then any time digits as zeros.
                let date = format!(
                    "{:04}{:02}{:02}",
                    1920_u16.saturating_add(u16::from(self.random.below(80)?)),
                    self.random.pick(1, 12)?,
                    self.random.pick(1, 28)?
                );
                for (index, slot) in fake.iter_mut().enumerate() {
                    *slot = date.as_bytes().get(index).copied().unwrap_or(b'0');
                }
            } else if matches!(widths.as_slice(), [4, 2, 2] | [2, 2, 4] | [2, 2, 2]) {
                // Every two-digit part is 01-12, valid as a day or a month in either order.
                for group in &groups {
                    let value = if group.len() == 4 {
                        format!(
                            "{:04}",
                            1920_u16.saturating_add(u16::from(self.random.below(80)?))
                        )
                    } else {
                        format!("{:02}", self.random.pick(1, 12)?)
                    };
                    let slots = fake.get_mut(group.clone()).ok_or(Error::Limit)?;
                    slots.copy_from_slice(value.as_bytes());
                }
            } else {
                return Ok(None);
            }
            if fake != real {
                return Ok(Some(fake));
            }
        }
        Err(Error::Limit)
    }
}

/// Whether a value has a byte its stand-in must change.
fn changeable(real: &[u8], keep: &[bool]) -> bool {
    real.iter()
        .zip(keep_mask(keep, real.len()))
        .any(|(byte, kept)| !kept && (byte.is_ascii_alphanumeric() || !byte.is_ascii()))
}

fn keep_mask(keep: &[bool], length: usize) -> impl Iterator<Item = bool> + '_ {
    (0..length).map(move |index| keep.get(index).copied().unwrap_or(false))
}

fn digit_groups(value: &[u8]) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let mut start = None;
    for (index, byte) in value.iter().enumerate() {
        match (byte.is_ascii_digit(), start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                groups.push(from..index);
                start = None;
            }
            (true, Some(_)) | (false, None) => (),
        }
    }
    if let Some(from) = start {
        groups.push(from..value.len());
    }
    groups
}

/// One same-shape random draw.
fn shaped(random: &mut Random, real: &[u8], keep: &[bool]) -> Result<Vec<u8>, Error> {
    let mask: Vec<bool> = keep_mask(keep, real.len()).collect();
    let free = |index: usize| !mask.get(index).copied().unwrap_or(true);
    // Leading zeros stay zeros; a value of zeros only changes its last zero.
    let zeros = real
        .iter()
        .enumerate()
        .take_while(|(index, byte)| **byte == b'0' && free(*index))
        .count();
    let all_zero_digits = real
        .iter()
        .enumerate()
        .filter(|(index, _)| free(*index))
        .all(|(_, byte)| !byte.is_ascii_alphanumeric() || *byte == b'0');
    let leading = if all_zero_digits && zeros > 0 {
        zeros.saturating_sub(1)
    } else {
        zeros
    };
    let text = std::str::from_utf8(real).ok();
    let mut fake = Vec::with_capacity(real.len());
    let mut index = 0_usize;
    while let Some(byte) = real.get(index).copied() {
        if !free(index) {
            fake.push(byte);
            index = index.saturating_add(1);
            continue;
        }
        let width = if byte.is_ascii() {
            1
        } else {
            text.and_then(|text| text.get(index..))
                .and_then(|rest| rest.chars().next())
                .map_or(1, char::len_utf8)
        };
        match (byte, width) {
            (b'0'..=b'9', _) if index < leading => fake.push(b'0'),
            (b'0'..=b'9', _) if index == leading => fake.push(random.pick(b'1', 9)?),
            (b'0'..=b'9', _) => fake.push(random.pick(b'0', 10)?),
            (b'A'..=b'Z', _) => fake.push(random.pick(b'A', 26)?),
            (b'a'..=b'z', _) => fake.push(random.pick(b'a', 26)?),
            (0x80..=0xFF, 2..=4) => fake.extend_from_slice(&wide(random, width)?),
            // A byte outside UTF-8 is a single-byte (Latin-1) letter.
            (0x80..=0xFF, _) => fake.push(latin1(random)?),
            (0..=0x7F, _) => fake.push(byte),
        }
        index = index.saturating_add(width);
    }
    Ok(fake)
}

/// A Latin-1 letter byte, U+00C0 to U+00FF without the two signs.
fn latin1(random: &mut Random) -> Result<u8, Error> {
    loop {
        let byte = random.pick(0xC0, 64)?;
        if byte != 0xD7 && byte != 0xF7 {
            return Ok(byte);
        }
    }
}

/// A letter whose UTF-8 encoding is `width` bytes long.
fn wide(random: &mut Random, width: usize) -> Result<Vec<u8>, Error> {
    let (from, count) = match width {
        2 => (0x00C0_u32, 64_u8),
        3 => (0x4E00, 128),
        _ => (0x2_0000, 128),
    };
    loop {
        let code = from.saturating_add(u32::from(random.below(count)?));
        if code == 0xD7 || code == 0xF7 {
            continue;
        }
        let character = char::from_u32(code).ok_or(Error::Limit)?;
        let mut buffer = [0_u8; 4];
        let encoded = character.encode_utf8(&mut buffer).as_bytes().to_vec();
        if encoded.len() == width {
            return Ok(encoded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0037: same length, same classes, leading zeros kept.
    #[test]
    fn stand_ins_keep_the_shape_and_leading_zeros() -> Result<(), Error> {
        let mut names = Pseudonyms::new([b"000PatA 12-x".to_vec()]);
        let fake = names.stand_in(Kind::Text, b"000PatA 12-x", &[])?;
        assert_eq!(fake.len(), 12);
        assert_eq!(fake.get(..3), Some(b"000".as_slice()));
        assert_ne!(fake.get(3), Some(&b'0'));
        assert!(
            fake.get(3..4)
                .is_some_and(|b| b.iter().all(u8::is_ascii_uppercase))
        );
        assert!(
            fake.get(4..6)
                .is_some_and(|b| b.iter().all(u8::is_ascii_lowercase))
        );
        assert!(fake.get(6).is_some_and(u8::is_ascii_uppercase));
        assert_eq!(fake.get(7), Some(&b' '));
        assert_eq!(fake.get(10), Some(&b'-'));
        assert_ne!(fake, b"000PatA 12-x");
        assert_eq!(names.stand_in(Kind::Text, b"000PatA 12-x", &[])?, fake);
        Ok(())
    }

    // ADR 0037: a value of zeros still changes; non-ASCII letters keep their width.
    #[test]
    fn zeros_and_wide_letters_still_change() -> Result<(), Error> {
        let mut names = Pseudonyms::new([]);
        let zeros = names.stand_in(Kind::Text, b"000", &[])?;
        assert_eq!(zeros.get(..2), Some(b"00".as_slice()));
        assert_ne!(zeros, b"000");
        let name = "Çelë".as_bytes();
        let fake = names.stand_in(Kind::Text, name, &[])?;
        assert_eq!(fake.len(), name.len());
        assert!(std::str::from_utf8(&fake).is_ok());
        assert_ne!(fake, name);
        Ok(())
    }

    // ADR 0037: birth dates stay valid dates of the same layout; sex becomes U.
    #[test]
    fn birth_dates_stay_dates_and_sex_becomes_unknown() -> Result<(), Error> {
        let mut names = Pseudonyms::new([]);
        let compact = names.stand_in(Kind::BirthDate, b"19800229", &[])?;
        let text = std::str::from_utf8(&compact).map_err(|_| Error::Limit)?;
        let year: u16 = text
            .get(..4)
            .and_then(|y| y.parse().ok())
            .ok_or(Error::Limit)?;
        let month: u8 = text
            .get(4..6)
            .and_then(|m| m.parse().ok())
            .ok_or(Error::Limit)?;
        let day: u8 = text
            .get(6..8)
            .and_then(|d| d.parse().ok())
            .ok_or(Error::Limit)?;
        assert!(
            (1920..2000).contains(&year) && (1..=12).contains(&month) && (1..=28).contains(&day)
        );
        let slashed = names.stand_in(Kind::BirthDate, b"2000/02/29", &[])?;
        assert_eq!(slashed.get(4), Some(&b'/'));
        assert_eq!(slashed.get(7), Some(&b'/'));
        assert_eq!(names.stand_in(Kind::Sex, b"F", &[])?, b"U");
        Ok(())
    }
}
