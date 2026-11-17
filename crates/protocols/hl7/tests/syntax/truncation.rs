//! Tests edition-specific truncation declarations, literal escaping and raw retention.
use analyte_hl7::*;

fn message(edition: &str, encoding: &str, value: &str) -> Vec<u8> {
    format!("MSH|{encoding}|S|F|R|F|202611161200||ORU^R01^ORU_R01|synthetic|P|{edition}\rOBX|1|ST|SYN||{value}\r").into_bytes()
}

#[test]
fn hl7_2_8_2_s2_14_9_2_and_2_9_1_s2_13_9_2_encoding_characters() {
    for edition in ["2.8.2", "2.9.1"] {
        for encoding in ["^~\\&", "^~\\&#", "^~\\&!"] {
            let raw = message(edition, encoding, "complete");
            let parsed = Message::parse(&raw, Options::default()).unwrap();
            assert_eq!(parsed.encode(), raw);
            assert_eq!(
                parsed.delimiters().truncation(),
                encoding.as_bytes().get(4).copied()
            );
        }
        for encoding in ["^~\\&^", "^~\\&|", "^~\\&A", "^~\\&##"] {
            assert!(
                Message::parse(&message(edition, encoding, "complete"), Options::default())
                    .is_err()
            );
        }
    }
    for edition in ["2.3", "2.3.1", "2.4", "2.5", "2.5.1", "2.6"] {
        assert_eq!(
            Message::parse(&message(edition, "^~\\&#", "complete"), Options::default()),
            Err(Error::Truncation)
        );
    }
}

#[test]
fn hl7_2_8_2_s2_5_5_2_and_2_9_1_s2_4_5_2_preserve_and_refuse_marked_values() {
    for edition in ["2.8.2", "2.9.1"] {
        for value in [
            "partial#",
            "partial#^other",
            "partial#&other",
            "partial#~other",
        ] {
            let raw = message(edition, "^~\\&#", value);
            let parsed = Message::parse(&raw, Options::default()).unwrap();
            let field = parsed.segments().nth(1).unwrap().field(5).unwrap();
            assert_eq!(field.raw(), value.as_bytes());
            assert!(field.is_truncated());
            assert_eq!(
                parsed.require_complete_values(),
                Err(Error::Truncated {
                    segment: 2,
                    field: 5
                })
            );
            assert_eq!(parsed.encode(), raw);
        }
    }
}

#[test]
fn hl7_2_9_1_s2_6_1_truncation_character_escape() {
    for edition in ["2.8.2", "2.9.1"] {
        let parsed = Message::parse(
            &message(edition, "^~\\&!", "literal\\P\\"),
            Options::default(),
        )
        .unwrap();
        let field = parsed.segments().nth(1).unwrap().field(5).unwrap();
        assert!(!field.is_truncated());
        assert_eq!(decode_text(field.raw(), &parsed).unwrap(), "literal!");
        assert!(parsed.require_complete_values().is_ok());
        assert_eq!(
            encode_text(b"literal!", &parsed, 10).unwrap(),
            b"literal\\P\\"
        );
        assert!(matches!(
            encode_text(b"literal!", &parsed, 9),
            Err(Error::Limit {
                kind: "text bytes",
                ..
            })
        ));
    }
}

#[test]
fn hl7_2_8_2_s2_7_2_undeclared_p_escape_remains_unsupported() {
    for edition in ["2.3.1", "2.5.1", "2.8.2", "2.9.1"] {
        let raw = message(edition, "^~\\&", "literal\\P\\");
        assert!(matches!(
            Message::parse(&raw, Options::default()),
            Err(Error::Escape { .. })
        ));
        let parsed = Message::parse(
            &raw,
            Options {
                escapes: EscapePolicy::Preserve,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(
            decode_text(b"literal\\P\\", &parsed).unwrap(),
            "literal\\P\\"
        );
        assert_eq!(encode_text(b"literal#", &parsed, 8).unwrap(), b"literal#");
    }
}
