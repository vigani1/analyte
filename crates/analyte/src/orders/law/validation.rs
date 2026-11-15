//! TF-2b Rev 11 B.3.1 (pp.63-64), B.7.6 (p.82): compare against retained work.
use crate::{Error, config::Machine, deadline};
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
                    std::str::from_utf8(awos.split(|b| *b == b'^').next().unwrap_or_default())?
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
                let Some(item) = record.items.iter().find(|item| {
                    raw(segment, 2).split(|b| *b == b'^').next() == Some(item.placer.as_bytes())
                }) else {
                    return Ok(Some(unknown));
                };
                let mut parts = awos.split(|b| *b == b'^');
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
                        b"OBR" if raw(original, 2) == item.placer.as_bytes() => {
                            found = true;
                            if specimen.zip(expected_spm).is_none_or(|(a, b)| {
                                !raw(b, 2).is_empty()
                                    && raw(b, 2) != b"\"\""
                                    && raw(a, 2) != raw(b, 2)
                            }) {
                                return Ok(Some(Mismatch {
                                    segment: b"SPM",
                                    occurrence: spm_count,
                                    field: 2,
                                }));
                            }
                            for field in [4_u16, 11] {
                                if specimen.zip(expected_spm).is_none_or(|(a, b)| {
                                    !same_code(
                                        raw(a, usize::from(field)),
                                        raw(b, usize::from(field)),
                                    )
                                }) {
                                    return Ok(Some(Mismatch {
                                        segment: b"SPM",
                                        occurrence: spm_count,
                                        field,
                                    }));
                                }
                            }
                            if container.zip(expected_sac).is_none_or(|(a, b)| {
                                raw(a, 3).split(|v| *v == b'^').next()
                                    != raw(b, 3).split(|v| *v == b'^').next()
                            }) {
                                return Ok(Some(Mismatch {
                                    segment: b"SAC",
                                    occurrence: sac_count,
                                    field: 3,
                                }));
                            }
                            if !same_code(raw(segment, 4), raw(original, 4)) {
                                return Ok(Some(Mismatch {
                                    segment: b"OBR",
                                    occurrence: obr_count,
                                    field: 4,
                                }));
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                if !found {
                    return Ok(Some(unknown));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

// Display text does not change a coded identifier; never equate different systems.
fn same_code(a: &[u8], b: &[u8]) -> bool {
    [0, 2, 3, 5].into_iter().all(|index| {
        a.split(|v| *v == b'^').nth(index).unwrap_or_default()
            == b.split(|v| *v == b'^').nth(index).unwrap_or_default()
    })
}
