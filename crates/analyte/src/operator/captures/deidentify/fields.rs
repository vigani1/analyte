//! Splits one delimited ASTM or HL7 field into the values a rule replaces, keeping escape sequences.
use std::ops::Range;

/// The delimiters inside a field, from the message's own header.
#[derive(Debug, Clone, Copy)]
pub(super) struct Delimiters {
    /// Repetition delimiter.
    pub(super) repeat: u8,
    /// Component delimiter.
    pub(super) component: u8,
    /// HL7 subcomponent delimiter; ASTM has none.
    pub(super) subcomponent: Option<u8>,
    /// Escape character.
    pub(super) escape: u8,
}

/// Which parts of a field hold patient values.
#[derive(Debug, Clone, Copy)]
pub(super) enum Select {
    /// Every component and subcomponent of every repetition.
    All,
    /// In every repetition, the first subcomponent of these components (1-based):
    /// the identifier of an EI or EIP value, not its assigning authority.
    First(&'static [usize]),
}

/// One value to replace.
#[derive(Debug)]
pub(super) struct Atom {
    /// Its bytes, relative to the field.
    pub(super) range: Range<usize>,
    /// Bytes of escape sequences, which stay as they are.
    pub(super) keep: Vec<bool>,
}

/// Escape sequences a replaced value may keep: delimiter and formatting escapes.
/// Hex, character-set and local escapes could spell patient text, so they refuse the file.
#[derive(Debug, Clone, Copy)]
pub(super) enum Escapes {
    /// LIS02-A2 §5.4.5.1: &F& &S& &R& &E&.
    Astm,
    /// HL7 2.5.1 ch.2 §2.7: \F\ \S\ \T\ \R\ \E\ \H\ \N\ and formatting such as \.br\.
    Hl7,
}

impl Escapes {
    fn allows(self, content: &[u8]) -> bool {
        match self {
            Self::Astm => matches!(content, b"F" | b"S" | b"R" | b"E"),
            Self::Hl7 => {
                matches!(content, b"F" | b"S" | b"T" | b"R" | b"E" | b"H" | b"N")
                    || (content.first() == Some(&b'.')
                        && content.iter().skip(1).all(|byte| {
                            byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'+'
                        }))
            }
        }
    }
}

/// Byte ranges of the pieces between `delimiter`s, relative to `bytes`.
pub(super) fn pieces(bytes: &[u8], delimiter: u8) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0_usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == delimiter {
            ranges.push(start..index);
            start = index.saturating_add(1);
        }
    }
    ranges.push(start..bytes.len());
    ranges
}

fn shift(range: &Range<usize>, by: usize) -> Range<usize> {
    range.start.saturating_add(by)..range.end.saturating_add(by)
}

/// The atoms `select` names in `field`.
/// # Errors
/// Refuses an escape sequence that could spell patient text.
pub(super) fn atoms(
    field: &[u8],
    delimiters: Delimiters,
    select: Select,
    escapes: Escapes,
) -> Result<Vec<Atom>, &'static str> {
    let mut found = Vec::new();
    for repetition in pieces(field, delimiters.repeat) {
        let text = field.get(repetition.clone()).unwrap_or_default();
        let components = pieces(text, delimiters.component);
        let mut chosen = Vec::new();
        match select {
            Select::All => {
                for component in &components {
                    let part = text.get(component.clone()).unwrap_or_default();
                    match delimiters.subcomponent {
                        Some(sub) => chosen.extend(
                            pieces(part, sub)
                                .iter()
                                .map(|piece| shift(piece, component.start)),
                        ),
                        None => chosen.push(component.clone()),
                    }
                }
            }
            Select::First(list) => {
                for number in list {
                    let Some(component) = components.get(number.saturating_sub(1)) else {
                        continue;
                    };
                    let part = text.get(component.clone()).unwrap_or_default();
                    let first = match delimiters.subcomponent {
                        Some(sub) => pieces(part, sub).into_iter().next().unwrap_or(0..0),
                        None => 0..part.len(),
                    };
                    chosen.push(shift(&first, component.start));
                }
            }
        }
        for range in chosen {
            let range = shift(&range, repetition.start);
            if range.is_empty() {
                continue;
            }
            let bytes = field.get(range.clone()).unwrap_or_default();
            let keep = escape_mask(bytes, delimiters.escape, escapes)?;
            found.push(Atom { range, keep });
        }
    }
    Ok(found)
}

/// Marks each complete escape sequence; a lone escape character is ordinary text.
fn escape_mask(bytes: &[u8], escape: u8, escapes: Escapes) -> Result<Vec<bool>, &'static str> {
    let mut keep = vec![false; bytes.len()];
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes.get(index) != Some(&escape) {
            index = index.saturating_add(1);
            continue;
        }
        let content_start = index.saturating_add(1);
        let Some(length) = bytes
            .get(content_start..)
            .and_then(|rest| rest.iter().position(|byte| *byte == escape))
        else {
            index = content_start;
            continue;
        };
        let end = content_start.saturating_add(length);
        if !escapes.allows(bytes.get(content_start..end).unwrap_or_default()) {
            return Err(analyte_i18n::text("cli-deidentify-field-escape"));
        }
        for slot in keep.get_mut(index..=end).unwrap_or_default() {
            *slot = true;
        }
        index = end.saturating_add(1);
    }
    Ok(keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HL7: Delimiters = Delimiters {
        repeat: b'~',
        component: b'^',
        subcomponent: Some(b'&'),
        escape: b'\\',
    };

    fn texts<'a>(field: &'a [u8], atoms: &[Atom]) -> Vec<&'a [u8]> {
        atoms
            .iter()
            .map(|atom| field.get(atom.range.clone()).unwrap_or_default())
            .collect()
    }

    // HL7 2.5.1 ch.2 §2.5.3 and §2A (EI, EIP): the identifier is the first (sub)component.
    #[test]
    fn identifier_rules_pick_only_the_identifier() -> Result<(), &'static str> {
        let field = b"S-1&LAB^F-2&LAB~S-3";
        let found = atoms(field, HL7, Select::First(&[1, 2]), Escapes::Hl7)?;
        assert_eq!(
            texts(field, &found),
            vec![b"S-1".as_slice(), b"F-2", b"S-3"]
        );
        let all = atoms(b"DOE^JO\\F\\N", HL7, Select::All, Escapes::Hl7)?;
        assert_eq!(all.len(), 2);
        assert_eq!(
            all.get(1).map(|a| a.keep.clone()),
            Some(vec![false, false, true, true, true, false])
        );
        Ok(())
    }

    // HL7 2.5.1 ch.2 §2.7: hex data escapes could spell any text.
    #[test]
    fn hex_escapes_in_patient_fields_are_refused() {
        assert!(atoms(b"\\X4A4F\\", HL7, Select::All, Escapes::Hl7).is_err());
    }
}
