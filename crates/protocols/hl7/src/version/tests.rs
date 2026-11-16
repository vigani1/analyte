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

#[test]
fn hl7_2_5_1_s2_8_recognizes_2_3_without_upgrading() {
    edition(Version::V23, "2.3", false, false);
}

#[test]
fn hl7_2_3_1_s2_24_1_2_7_12_recognizes_edition() {
    edition(Version::V231, "2.3.1", false, false);
}

#[test]
fn hl7_2_5_1_s2_8_and_s2_15_9_7_recognizes_2_4() {
    edition(Version::V24, "2.4", true, false);
}

#[test]
fn hl7_2_5_1_s2_8_recognizes_2_5_without_patch_upgrade() {
    edition(Version::V25, "2.5", true, false);
}

#[test]
fn hl7_2_5_1_s2_15_9_2_7_12_recognizes_edition() {
    edition(Version::V251, "2.5.1", true, false);
}

#[test]
fn hl7_2_6_s2_14_9_2_7_12_recognizes_edition_without_typed_fallback() {
    edition(Version::V26, "2.6", true, false);
}

#[test]
fn hl7_2_8_2_s2_14_9_2_7_12_recognizes_edition_without_typed_fallback() {
    edition(Version::V282, "2.8.2", true, true);
}

#[test]
fn hl7_2_9_1_s2_13_9_2_7_12_recognizes_edition_without_typed_fallback() {
    edition(Version::V291, "2.9.1", true, true);
}

#[test]
fn adr0012_s3_no_unlisted_edition_or_prefix_match() {
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

#[test]
fn adr0012_s3_writer_qualification_is_separate_from_receiving() {
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
