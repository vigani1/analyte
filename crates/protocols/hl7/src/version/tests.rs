//! Tests exact edition recognition without implicit semantic upgrades.
use super::*;
use crate::{Message, Msh, Options};

fn edition(edition: Version, wire: &str, required_time: bool, truncation: bool) {
    assert_eq!(Version::parse(wire.as_bytes()), Some(edition));
    assert_eq!(edition.as_str(), wire);
    assert_eq!(edition.required_timestamp(), required_time);
    for encoding in ["^~\\&", "^~\\&#"] {
        let raw =
            format!("MSH|{encoding}|S|F|R|F|202611161200||ORU^R01^ORU_R01|synthetic|P|{wire}\r");
        let result = Message::parse(raw.as_bytes(), Options::default());
        if encoding.len() == 5 && !truncation {
            assert_eq!(result, Err(Error::Truncation));
        } else {
            let message = result.unwrap();
            assert_eq!(message.raw(), raw.as_bytes());
            assert_eq!(Msh::new(&message).unwrap().version(), edition);
        }
    }
}

// HL7 2.5.1 §2.8
#[test]
fn recognizes_v23_without_upgrading() {
    edition(Version::V23, "2.3", false, false);
}

// HL7 2.3.1 §2.24.1.2, §2.24.1.7, §2.24.1.12
#[test]
fn recognizes_v231() {
    edition(Version::V231, "2.3.1", false, false);
}

// HL7 2.5.1 §2.8, §2.15.9.7
#[test]
fn recognizes_v24() {
    edition(Version::V24, "2.4", true, false);
}

// HL7 2.5.1 §2.8
#[test]
fn recognizes_v25_without_patch_upgrade() {
    edition(Version::V25, "2.5", true, false);
}

// HL7 2.5.1 §2.15.9.2, §2.15.9.7, §2.15.9.12
#[test]
fn recognizes_v251() {
    edition(Version::V251, "2.5.1", true, false);
}

// HL7 2.6 §2.14.9.2, §2.14.9.7, §2.14.9.12
#[test]
fn recognizes_v26_without_typed_fallback() {
    edition(Version::V26, "2.6", true, false);
}

// HL7 2.8.2 §2.14.9.2, §2.14.9.7, §2.14.9.12
#[test]
fn recognizes_v282_without_typed_fallback() {
    edition(Version::V282, "2.8.2", true, true);
}

// HL7 2.9.1 §2.13.9.2, §2.13.9.7, §2.13.9.12
#[test]
fn recognizes_v291_without_typed_fallback() {
    edition(Version::V291, "2.9.1", true, true);
}

// ADR 0012 §3
#[test]
fn unlisted_edition_or_prefix_is_refused() {
    for raw in [
        "", "2", "2.2", "2.3.0", "2.5.10", "2.7", "2.8", "2.9", "2.9.2", "2.9.1 ",
    ] {
        assert_eq!(Version::parse(raw.as_bytes()), None);
        let wire = format!("MSH|^~\\&|S|F|R|F|202611161200||ORU^R01^ORU_R01|synthetic|P|{raw}\r");
        assert_eq!(
            Message::parse(wire.as_bytes(), Options::default()),
            Err(Error::Control {
                segment: "MSH",
                field: 12
            })
        );
    }
}

// ADR 0012 §3
#[test]
fn writer_qualification_separate_from_receiving() {
    for (version, reason) in [
        (
            Version::V23,
            "original 2.3 order field definitions are unavailable",
        ),
        (
            Version::V24,
            "original 2.4 order field definitions are unavailable",
        ),
        (
            Version::V25,
            "original 2.5 order field definitions are unavailable",
        ),
        (Version::V291, "section 4.5.1.4"),
    ] {
        assert!(version.supports_result_reception());
        assert!(version.order_sending_blocker().unwrap().contains(reason));
    }
    for version in [Version::V231, Version::V251, Version::V26, Version::V282] {
        assert_eq!(version.order_sending_blocker(), None);
    }
}

// HL7 chapters 2, 4, 5, 7
#[test]
fn edition_capabilities_are_pinned() {
    let rows = [
        (
            Version::V23,
            [
                false, false, false, false, false, false, false, false, false, false, false,
            ],
        ),
        (
            Version::V231,
            [
                false, false, false, false, false, true, false, false, false, false, false,
            ],
        ),
        (
            Version::V24,
            [
                false, false, false, false, false, false, false, false, false, false, false,
            ],
        ),
        (
            Version::V25,
            [
                true, false, false, false, true, false, false, false, false, false, false,
            ],
        ),
        (
            Version::V251,
            [
                true, false, false, false, true, false, true, true, true, true, true,
            ],
        ),
        (
            Version::V26,
            [
                true, true, false, false, true, false, true, false, false, true, false,
            ],
        ),
        (
            Version::V282,
            [
                true, true, true, false, false, false, true, false, false, true, false,
            ],
        ),
        (
            Version::V291,
            [
                true, true, true, true, false, false, false, false, false, true, false,
            ],
        ),
    ];
    for (version, expected) in rows {
        assert_eq!(
            [
                version.modern_error_layout(),
                version.fatal_error_severity(),
                version.enhanced_result_identity(),
                version.paired_application_ack(),
                version.legacy_query_structure(),
                version.orm_patient_name_required(),
                version.oml_writer(),
                version.legacy_cwe(),
                version.oul_reception(),
                version.analysis_time(),
                version.law_edition(),
            ],
            expected,
            "{}",
            version.as_str()
        );
    }
}

// HL7 chapters 2, 4, 5, 7
#[test]
fn each_independent_edition_rule_is_pinned() {
    for (version, modern, legacy_order, later, query) in [
        (Version::V23, false, false, false, Some((60, 9))),
        (Version::V231, false, true, false, Some((60, 9))),
        (Version::V24, false, false, false, Some((60, 9))),
        (Version::V25, true, false, false, Some((60, 9))),
        (Version::V251, true, false, false, Some((250, 10))),
        (Version::V26, true, false, false, Some((250, 10))),
        (Version::V282, true, false, true, None),
        (Version::V291, true, false, true, None),
    ] {
        assert_eq!(version.ack_structure(), modern);
        assert_eq!(version.orm_writer(), legacy_order);
        assert_eq!(version.placer_position(), legacy_order);
        assert_eq!(version.utf8_departure(), legacy_order);
        assert_eq!(version.single_order_error(), legacy_order);
        assert_eq!(version.short_patient_identifier(), legacy_order);
        assert_eq!(version.specimen_code_version(), !legacy_order);
        assert_eq!(version.unknown_key_error(), later);
        assert_eq!(version.report_status_time(), later);
        assert_eq!(
            version
                .query_person_bytes()
                .ok()
                .zip(version.query_filter_fields().ok()),
            query
        );
    }
}

#[test]
fn config_keys_pin_all_hl7_editions_without_wire_fallback() {
    for (edition, key) in [
        (Version::V23, "v23"),
        (Version::V231, "v231"),
        (Version::V24, "v24"),
        (Version::V25, "v25"),
        (Version::V251, "v251"),
        (Version::V26, "v26"),
        (Version::V282, "v282"),
        (Version::V291, "v291"),
    ] {
        assert_eq!(edition.config_key(), key);
        assert_eq!(Version::from_config_key(key), Some(edition));
        assert_eq!(Version::from_config_key(edition.as_str()), None);
    }
    assert_eq!(Version::from_config_key("V251"), None);
    assert_eq!(Version::from_config_key("v27"), None);
}

// Exhaustive: a new edition does not compile here until it joins this chain,
// and the test below then requires its single row in `EDITIONS`.
fn successor(edition: Version) -> Option<Version> {
    match edition {
        Version::V23 => Some(Version::V231),
        Version::V231 => Some(Version::V24),
        Version::V24 => Some(Version::V25),
        Version::V25 => Some(Version::V251),
        Version::V251 => Some(Version::V26),
        Version::V26 => Some(Version::V282),
        Version::V282 => Some(Version::V291),
        Version::V291 => None,
    }
}

#[test]
fn every_edition_has_exactly_one_row_in_the_edition_table() {
    let editions = std::iter::successors(Some(Version::V23), |edition| successor(*edition))
        .collect::<Vec<_>>();
    assert_eq!(EDITIONS.len(), editions.len());
    for edition in editions {
        let rows = EDITIONS
            .iter()
            .filter(|row| row.edition == edition)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1, "{edition:?}");
        assert!(rows.iter().all(|row| row.wire == edition.entry().wire));
        assert_eq!(edition.entry().edition, edition);
        assert_eq!(
            Version::from_config_key(edition.config_key()),
            Some(edition)
        );
        assert_eq!(Version::parse(edition.as_str().as_bytes()), Some(edition));
    }
}
