//! Asserts exact LAW specimen and container rendering against supplied fixture values.
use super::*;

pub(super) fn assert_supplied_details(output: &Message) -> TestResult {
    for (id, fields) in [
        (
            b"SPM".as_slice(),
            vec![
                (2, b"sample&99LAB".as_slice()),
                (3, b"parent&Local"),
                (7, b"id^^HL70488"),
                (8, b"id^^99LAB"),
                (9, b"id^^99LAB"),
                (16, b"id^^HL70489"),
                (17, b"20240101120000"),
                (18, b"20240101130000"),
                (27, b"id^^99LAB"),
            ],
        ),
        (
            b"SAC".as_slice(),
            vec![
                (3, b"".as_slice()),
                (4, b"parent"),
                (9, b"id^^HL70378"),
                (10, b"rack"),
                (11, b"1^2"),
                (13, b"tray"),
                (14, b"1^2"),
                (15, b"location^^99LAB"),
                (21, b"2.5"),
                (22, b"1.2"),
                (24, b"mL^Milliliters^UCUM"),
                (29, b"^1^:^5.5"),
            ],
        ),
    ] {
        let segment = output
            .segments()
            .find(|s| s.id() == id)
            .ok_or("detail segment")?;
        for (number, expected) in fields {
            assert_eq!(
                segment
                    .field(number)
                    .map_or(b"".as_slice(), analyte_hl7::Field::raw),
                expected,
                "{id:?}-{number}"
            );
        }
    }
    Ok(())
}
pub(super) fn assert_omitted_details(minimal: &Message) -> TestResult {
    for (id, fields) in [
        (b"SPM".as_slice(), vec![3, 7, 8, 9, 16, 17, 18, 27]),
        (
            b"SAC".as_slice(),
            vec![9, 10, 11, 13, 14, 15, 21, 22, 24, 29],
        ),
    ] {
        let segment = minimal
            .segments()
            .find(|s| s.id() == id)
            .ok_or("minimal segment")?;
        for number in fields {
            assert_eq!(
                segment
                    .field(number)
                    .map_or(b"".as_slice(), analyte_hl7::Field::raw),
                b"",
                "invented {id:?}-{number}"
            );
        }
    }
    Ok(())
}

pub(super) fn assert_optional_details_omitted(
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
) -> TestResult {
    use law::{Awos, WorkAction};
    let minimal_awos = Awos::new(
        b"awos-2",
        b"sample&99LAB",
        b"SER^^HL70487",
        b"P",
        b"tube",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?;
    let minimal = law::work_order(
        &[minimal_awos],
        None,
        address,
        response,
        capabilities,
        options(),
    )?;
    assert_omitted_details(&minimal)?;
    Ok(())
}
