//! Tests autolyser field preservation and malformed inputs against vendor source sections.
use super::*;
use proptest::prelude::*;

#[test]
fn autolyser_rev18_section23_1_p175_every_worklist_field_and_limits() {
    for list in [List::Routine, List::Stat] {
        for sample in [Sample::Serum, Sample::Urine] {
            for clone in [false, true] {
                for position in [0, 99] {
                    for count in [1, 99] {
                        let patient = PatientCode::from_eight(b"000042aZ").unwrap();
                        let tests = vec![Field::new(b" A1 ").unwrap(); count];
                        let order = Worklist::new(
                            patient.clone(),
                            list,
                            sample,
                            clone,
                            position,
                            tests.clone(),
                        )
                        .unwrap();
                        let parsed = Worklist::parse(&order.encode()).unwrap();
                        assert_eq!(parsed, order);
                        assert_eq!(parsed.patient(), &patient);
                        assert_eq!(parsed.list(), list);
                        assert_eq!(parsed.sample(), sample);
                        assert_eq!(parsed.is_clone(), clone);
                        assert_eq!(parsed.position(), position);
                        assert_eq!(parsed.tests(), tests);
                    }
                }
            }
        }
    }
    let patient = PatientCode::from_eight(b"000042aZ").unwrap();
    for (position, count) in [(100, 1), (0, 0), (0, 100)] {
        assert!(
            Worklist::new(
                patient.clone(),
                List::Routine,
                Sample::Serum,
                false,
                position,
                vec![Field::new(b"TEST").unwrap(); count]
            )
            .is_err()
        );
    }
}

#[test]
fn autolyser_rev18_section23_2_pp177_178_all_report_and_control_fields_roundtrip() {
    let patient = PatientCode::from_eight(b"000042aZ").unwrap();
    let values = vec![
        Value {
            code: Field::new(b" A1 ").unwrap(),
            text: Field::new(b" <0.100").unwrap(),
        },
        Value {
            code: Field::new(b" A1 ").unwrap(),
            text: Field::new(b"1,234  ").unwrap(),
        },
    ];
    for list in [List::Routine, List::Stat] {
        for sample in [Sample::Serum, Sample::Urine] {
            for control in [
                None,
                Some(Control {
                    is_control: false,
                    known: true,
                    level: Field::new(b"001").unwrap(),
                    lot: Field::new(b"                    ").unwrap(),
                }),
                Some(Control {
                    is_control: true,
                    known: false,
                    level: Field::new(b"002").unwrap(),
                    lot: Field::new(b"SYNTHETIC LOT       ").unwrap(),
                }),
                Some(Control {
                    is_control: true,
                    known: true,
                    level: Field::new(b"003").unwrap(),
                    lot: Field::new(b"SYNTHETIC LOT       ").unwrap(),
                }),
            ] {
                let report =
                    Report::new(&patient, list, sample, control.as_ref(), &values).unwrap();
                let parsed = Report::parse(&report.raw, control.is_some()).unwrap();
                assert_eq!(parsed, report);
                assert_eq!(parsed.patient, patient);
                assert_eq!(parsed.list, list);
                assert_eq!(parsed.sample, sample);
                assert_eq!(parsed.control, control);
                assert_eq!(parsed.values, values);
            }
        }
    }
    for count in [0, 999] {
        let report = Report::new(
            &patient,
            List::Routine,
            Sample::Serum,
            None,
            &vec![values[0].clone(); count],
        )
        .unwrap();
        assert_eq!(report.values.len(), count);
    }
    assert_eq!(
        Report::new(
            &patient,
            List::Routine,
            Sample::Serum,
            None,
            &vec![values[0].clone(); 1000]
        ),
        Err(Error::Count)
    );
}

#[test]
fn autolyser_rev18_section23_pp175_179_malformed_fields_counts_and_lengths() {
    let order = Worklist::new(
        PatientCode::from_eight(b"000042aZ").unwrap(),
        List::Routine,
        Sample::Serum,
        false,
        1,
        vec![Field::new(b"TEST").unwrap()],
    )
    .unwrap()
    .encode();
    for length in 0..order.len() {
        assert!(Worklist::parse(&order[..length]).is_err());
    }
    for offset in [0, 15, 16, 17, 18, 20, 22] {
        let mut raw = order.clone();
        raw[offset] = 0;
        assert!(Worklist::parse(&raw).is_err(), "{offset}");
    }
    let mut raw = order;
    raw.push(b' ');
    assert!(Worklist::parse(&raw).is_err());
    let patient = PatientCode::from_eight(b"000042aZ").unwrap();
    let control = Control {
        is_control: false,
        known: false,
        level: Field::new(b"001").unwrap(),
        lot: Field::new(b"                    ").unwrap(),
    };
    assert!(Report::new(&patient, List::Routine, Sample::Serum, Some(&control), &[]).is_err());
    let report = Report::new(
        &patient,
        List::Routine,
        Sample::Serum,
        None,
        &[Value {
            code: Field::new(b"TEST").unwrap(),
            text: Field::new(b"error  ").unwrap(),
        }],
    )
    .unwrap();
    for length in 0..report.raw.len() {
        assert!(Report::parse(&report.raw[..length], false).is_err());
    }
    for offset in [0, 15, 16, 17, 20, 24] {
        let mut raw = report.raw.clone();
        raw[offset] = 0;
        assert!(Report::parse(&raw, false).is_err(), "{offset}");
    }
    assert!(Field::<4>::new(b"ABC").is_err());
    assert!(Field::<4>::new(b"ABC\n").is_err());
    assert!(Worklist::parse(&vec![b' '; 11035]).is_err());
    assert!(Report::parse(&vec![b' '; 11035], true).is_err());
}

#[test]
fn autolyser_rev18_section23_2_p178_control_discriminators_and_ordinary_layout_rules() {
    let patient = PatientCode::from_eight(b"000042aZ").unwrap();
    let control = Control {
        is_control: false,
        known: true,
        level: Field::new(b"001").unwrap(),
        lot: Field::new(b"                    ").unwrap(),
    };
    let report = Report::new(&patient, List::Routine, Sample::Serum, Some(&control), &[]).unwrap();
    for (offset, byte) in [(17, b'?'), (18, b'?'), (19, b'x'), (21, b'2'), (22, b'X')] {
        let mut raw = report.raw.clone();
        raw[offset] = byte;
        assert!(Report::parse(&raw, true).is_err(), "{offset}");
    }
    let value = Value {
        code: Field::new(b"TEST").unwrap(),
        text: Field::new(b"exact  ").unwrap(),
    };
    let maximum = Report::new(
        &patient,
        List::Stat,
        Sample::Urine,
        Some(&control),
        &vec![value; 999],
    )
    .unwrap();
    assert_eq!(maximum.raw.len(), 11034);
    assert_eq!(Report::parse(&maximum.raw, true).unwrap(), maximum);
}

proptest! {
    #[test]
    fn autolyser_rev18_section23_1_p175_worklist_roundtrip(patient in "[ -~]{15}", codes in prop::collection::vec("[ -~]{4}",1..100), position in 0u8..100, clone in any::<bool>()) {
        let tests = codes.iter().map(|code| Field::new(code.as_bytes()).unwrap()).collect();
        let order = Worklist::new(PatientCode::new(patient.as_bytes()).unwrap(), List::Stat, Sample::Urine, clone, position, tests).unwrap();
        prop_assert_eq!(Worklist::parse(&order.encode()).unwrap(), order);
    }
    #[test]
    fn autolyser_rev18_section23_2_p178_report_roundtrip(value in "[ -~]{7}") {
        let patient = PatientCode::from_eight(b"000042aZ").unwrap();
        let report = Report::new(&patient, List::Routine, Sample::Serum, None, &[Value { code: Field::new(b"TEST").unwrap(), text: Field::new(value.as_bytes()).unwrap() }]).unwrap();
        prop_assert_eq!(Report::parse(&report.raw, false).unwrap(), report);
    }
}
