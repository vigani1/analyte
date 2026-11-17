//! Reads returned LAW work and compares immutable order context (TF-2b B.3.1 and B.7.6).
use crate::{Message, Segment};
/// Location of a returned work mismatch, preserving source occurrence.
#[derive(Clone, Copy)]
pub struct Mismatch {
    /// Segment identifier.
    pub segment: &'static [u8],
    /// One-based segment occurrence.
    pub occurrence: u16,
    /// One-based field position.
    pub field: u16,
}
fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", crate::Field::raw)
}

fn components(segment: Segment<'_>, field: usize) -> impl Iterator<Item = &[u8]> {
    segment
        .field(field)
        .into_iter()
        .flat_map(crate::Field::repetitions)
        .flat_map(crate::Repetition::components)
        .map(crate::Component::raw)
}

/// Returned order context, preserving the enclosing specimen and container.
pub struct ReturnedWork<'a> {
    specimen: Option<Segment<'a>>,
    container: Option<Segment<'a>>,
    segment: Segment<'a>,
    spm_count: u16,
    sac_count: u16,
    obr_count: u16,
}

/// Compares identity and coded context against the immutable sent command.
#[must_use]
pub fn compare_retained(
    original: &Message,
    placer: &str,
    returned: &ReturnedWork<'_>,
    unknown: Mismatch,
) -> Option<Mismatch> {
    let &ReturnedWork {
        specimen,
        container,
        segment,
        spm_count,
        sac_count,
        obr_count,
    } = returned;
    let mut expected_spm = None;
    let mut expected_sac = None;
    let mut found = false;
    for original in original.segments() {
        match original.id() {
            b"SPM" => {
                expected_spm = Some(original);
                expected_sac = None;
            }
            b"SAC" => expected_sac = Some(original),
            b"OBR" if raw(original, 2) == placer.as_bytes() => {
                found = true;
                if specimen.zip(expected_spm).is_none_or(|(a, b)| {
                    !raw(b, 2).is_empty() && raw(b, 2) != b"\"\"" && raw(a, 2) != raw(b, 2)
                }) {
                    return Some(Mismatch {
                        segment: b"SPM",
                        occurrence: spm_count,
                        field: 2,
                    });
                }
                for field in [4_u16, 11] {
                    if specimen
                        .zip(expected_spm)
                        .is_none_or(|(a, b)| !same_code(a, b, usize::from(field)))
                    {
                        return Some(Mismatch {
                            segment: b"SPM",
                            occurrence: spm_count,
                            field,
                        });
                    }
                }
                if container
                    .zip(expected_sac)
                    .is_none_or(|(a, b)| components(a, 3).next() != components(b, 3).next())
                {
                    return Some(Mismatch {
                        segment: b"SAC",
                        occurrence: sac_count,
                        field: 3,
                    });
                }
                if !same_code(segment, original, 4) {
                    return Some(Mismatch {
                        segment: b"OBR",
                        occurrence: obr_count,
                        field: 4,
                    });
                }
                break;
            }
            _ => {}
        }
    }
    if !found {
        return Some(unknown);
    }
    None
}

// Display text does not change a coded identifier; never equate different systems.
fn same_code(a: Segment<'_>, b: Segment<'_>, field: usize) -> bool {
    [0, 2, 3, 5].into_iter().all(|index| {
        components(a, field).nth(index).unwrap_or_default()
            == components(b, field).nth(index).unwrap_or_default()
    })
}

impl ReturnedWork<'_> {
    /// Returns the explicit EI.1 placer, excluding the LAW null marker.
    #[must_use]
    pub fn placer(&self) -> Option<&[u8]> {
        (raw(self.segment, 2) != b"\"\"")
            .then(|| components(self.segment, 2).next().unwrap_or_default())
    }
    /// Returns the original OBR-2 location for unknown work.
    #[must_use]
    pub fn unknown(&self) -> Mismatch {
        Mismatch {
            segment: b"OBR",
            occurrence: self.obr_count,
            field: 2,
        }
    }
}
/// Returns bounded work contexts without consulting the host cache.
///
/// # Errors
/// Returns the segment whose occurrence cannot be represented.
pub fn returned_work(message: &Message) -> Result<Vec<ReturnedWork<'_>>, crate::Error> {
    let (mut specimen, mut container) = (None, None);
    let (mut spm_count, mut sac_count, mut obr_count) = (0_u16, 0_u16, 0_u16);
    let mut work = Vec::new();
    for segment in message.segments() {
        match segment.id() {
            b"SPM" => {
                spm_count = spm_count.checked_add(1).ok_or(crate::Error::Control {
                    segment: "SPM",
                    field: 0,
                })?;
                specimen = Some(segment);
                container = None;
            }
            b"SAC" => {
                sac_count = sac_count.checked_add(1).ok_or(crate::Error::Control {
                    segment: "SAC",
                    field: 0,
                })?;
                container = Some(segment);
            }
            b"OBR" => {
                obr_count = obr_count.checked_add(1).ok_or(crate::Error::Control {
                    segment: "OBR",
                    field: 0,
                })?;
                work.push(ReturnedWork {
                    specimen,
                    container,
                    segment,
                    spm_count,
                    sac_count,
                    obr_count,
                });
            }
            _ => {}
        }
    }
    Ok(work)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;

    // HL7 2.5.1 §§2.5.4/2.6: each field uses its own message's delimiters.
    #[test]
    fn hl7_251_2_5_4_2_6_retained_work_components_use_each_message_delimiters()
    -> Result<(), Box<dyn std::error::Error>> {
        let default = Message::parse(b"MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|m1|P|2.5.1\rOBR||awos-01^LAB||SYN^Display^99LAB\r", Options::default())?;
        let changed = Message::parse(b"MSH|$~\\&|A|AF|M|MF|20260101120000+0000||OUL$R22$OUL_R22|m2|P|2.5.1\rOBR||awos-01$LAB||SYN$Other display$99LAB\r", Options::default())?;
        let a = default
            .segments()
            .find(|s| s.id() == b"OBR")
            .ok_or("synthetic OBR missing")?;
        let b = changed
            .segments()
            .find(|s| s.id() == b"OBR")
            .ok_or("synthetic OBR missing")?;
        assert_eq!(components(b, 2).next(), Some(b"awos-01".as_slice()));
        assert_eq!(components(b, 2).nth(1), Some(b"LAB".as_slice()));
        assert!(same_code(a, b, 4));
        Ok(())
    }
}
