//! Validates LAW work replies against retained orders (TF-2b B.3.1 and B.7.6).
use crate::{Error, deadline, installation::config::Machine};
use analyte_hl7::{Message, Options, Segment, law};
use analyte_store::Handle;

#[derive(Clone, Copy)]
pub(crate) struct Mismatch {
    pub(crate) segment: &'static [u8],
    pub(crate) occurrence: u16,
    pub(crate) field: u16,
}
fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", analyte_hl7::Field::raw)
}

fn components(segment: Segment<'_>, field: usize) -> impl Iterator<Item = &[u8]> {
    segment
        .field(field)
        .into_iter()
        .flat_map(analyte_hl7::Field::repetitions)
        .flat_map(analyte_hl7::Repetition::components)
        .map(analyte_hl7::Component::raw)
}

pub(crate) async fn check(
    store: &Handle,
    machine: &Machine,
    message: &Message,
    options: Options,
) -> Result<Option<Mismatch>, Error> {
    let mut specimen = None;
    let mut container = None;
    let mut spm_count = 0_u16;
    let mut sac_count = 0_u16;
    let mut obr_count = 0_u16;
    for segment in message.segments() {
        match segment.id() {
            b"SPM" => {
                spm_count = spm_count.checked_add(1).ok_or("SPM occurrence overflow")?;
                specimen = Some(segment);
                container = None;
            }
            b"SAC" => {
                sac_count = sac_count.checked_add(1).ok_or("SAC occurrence overflow")?;
                container = Some(segment);
            }
            b"OBR" => {
                obr_count = obr_count.checked_add(1).ok_or("OBR occurrence overflow")?;
                let awos = raw(segment, 2);
                if awos == b"\"\"" {
                    continue;
                }
                let placer =
                    std::str::from_utf8(components(segment, 2).next().unwrap_or_default())?
                        .to_owned();
                let target = machine.id.clone();
                let record = store
                    .request(deadline(), move |s| {
                        s.machine_order_by_placer(&target, &placer)
                    })
                    .await?;
                let unknown = Mismatch {
                    segment: b"OBR",
                    occurrence: obr_count,
                    field: 2,
                };
                let Some(record) = record else {
                    return Ok(Some(unknown));
                };
                let Some(item) = record
                    .items
                    .iter()
                    .find(|item| components(segment, 2).next() == Some(item.placer.as_bytes()))
                else {
                    return Ok(Some(unknown));
                };
                let mut parts = components(segment, 2);
                parts.next();
                let namespace = parts.next().unwrap_or_default();
                if !namespace.is_empty()
                    && Some(namespace)
                        != item
                            .placer_namespace
                            .as_ref()
                            .map(|n| n.as_str().as_bytes())
                    || parts.any(|p| !p.is_empty())
                {
                    return Ok(Some(unknown));
                }
                let Some(bytes) = record.original_command() else {
                    return Ok(Some(unknown));
                };
                let original = law::parse_message(bytes, options)?;
                if let Some(mismatch) = compare_retained(
                    &original,
                    item.placer.as_str(),
                    &ReturnedWork {
                        specimen,
                        container,
                        segment,
                        spm_count,
                        sac_count,
                        obr_count,
                    },
                    unknown,
                ) {
                    return Ok(Some(mismatch));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

struct ReturnedWork<'a> {
    specimen: Option<Segment<'a>>,
    container: Option<Segment<'a>>,
    segment: Segment<'a>,
    spm_count: u16,
    sac_count: u16,
    obr_count: u16,
}

fn compare_retained(
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

#[cfg(test)]
mod tests {
    use super::*;

    // HL7 2.5.1 §§2.5.4/2.6: each field uses its own message's delimiters.
    #[test]
    fn hl7_251_2_5_4_2_6_retained_work_components_use_each_message_delimiters() -> Result<(), Error>
    {
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
