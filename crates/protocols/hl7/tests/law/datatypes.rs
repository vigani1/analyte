//! Regression tests for HL7 2.5.1 ERL, SN and LAW text-kind boundaries.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection, Error, Profile},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn fixture(
    id: &str,
    changes: &[(usize, &str)],
    direction: ControlDirection,
) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = [""; 40];
    let defaults = match id {
        "PID" => vec![(3, "000Ab"), (5, "Family^Given^^^^^L")],
        "ERR" => vec![(2, "OBX^1^5"), (3, "100^^HL70357"), (4, "E")],
        "NTE" => vec![
            (1, "1"),
            (
                2,
                if direction == ControlDirection::Send {
                    "A"
                } else {
                    "Z"
                },
            ),
            (3, "note"),
        ],
        "OBX" => vec![
            (1, "1"),
            (2, "ST"),
            (3, "test^Synthetic^99LAB"),
            (4, "1"),
            (5, "value"),
            (8, "N^^HL70078"),
            (11, "F"),
            (16, "operator"),
            (18, "model^maker~serial^maker"),
            (19, "20260101120000"),
            (29, "RSLT"),
        ],
        _ => return Err("unknown fixture".into()),
    };
    for (field, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(field).ok_or("fixture field")? = value;
    }
    let raw = format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\r{id}|{}\r",
        fields.get(1..).ok_or("fixture fields")?.join("|")
    );
    Ok(law::parse_message(
        raw.as_bytes(),
        Options {
            charset: Charset::Utf8,
            ..Options::default()
        },
    )?)
}
fn validate(
    id: &str,
    changes: &[(usize, &str)],
    direction: ControlDirection,
) -> Result<Result<(), Error>, Box<dyn std::error::Error>> {
    let message = fixture(id, changes, direction)?;
    let raw = message.raw().to_vec();
    let result =
        Profile::receive().validate_segment(message.segments().nth(1).ok_or("segment")?, direction);
    assert_eq!(message.raw(), raw);
    Ok(result)
}
#[test]
fn hl7_251_2_a_28_4_law_c1_2_repetition_requires_field_position() -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(matches!(
            validate("ERR", &[(2, "OBX^1^^2")], direction)?,
            Err(Error::Component {
                segment: "ERR",
                field: 2,
                component: 4
            })
        ));
        for value in ["OBX^1", "OBX^1^5", "OBX^1^5^1", "OBX^01^05^01"] {
            assert!(
                validate("ERR", &[(2, value)], direction)?.is_ok(),
                "{value}"
            );
        }
    }
    Ok(())
}
#[test]
fn hl7_251_2_a_70_law_c7_11_two_numbers_require_separator() -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(matches!(
            validate(
                "OBX",
                &[(2, "SN"), (6, "mg/L^mg/L^UCUM"), (5, "<^1^^2")],
                direction
            )?,
            Err(Error::Component {
                segment: "OBX",
                field: 5,
                component: 3
            })
        ));
        for separator in ["-", "+", "/", ".", ":"] {
            assert!(
                validate(
                    "OBX",
                    &[
                        (2, "SN"),
                        (6, "mg/L^mg/L^UCUM"),
                        (5, &format!("=^+0001.00^{separator}^-0.00"))
                    ],
                    direction
                )?
                .is_ok()
            );
        }
        for value in ["<^0.1", "<^1^+", "=^-0.00^."] {
            assert!(
                validate(
                    "OBX",
                    &[(2, "SN"), (6, "mg/L^mg/L^UCUM"), (5, value)],
                    direction
                )?
                .is_ok()
            );
        }
    }
    Ok(())
}
fn text_cases(
    value: &str,
    direction: ControlDirection,
    st: bool,
    tx: bool,
    ft: bool,
) -> TestResult {
    for (id, changes, expected) in [
        ("PID", vec![(5, format!("{value}^^^^^^L"))], st),
        ("OBX", vec![(2, "ST".to_owned()), (5, value.to_owned())], st),
        ("OBX", vec![(2, "TX".to_owned()), (5, value.to_owned())], tx),
        ("ERR", vec![(8, value.to_owned())], tx),
        ("NTE", vec![(3, value.to_owned())], ft),
    ] {
        let borrowed: Vec<_> = changes.iter().map(|(n, v)| (*n, v.as_str())).collect();
        let result = validate(id, &borrowed, direction)?;
        assert_eq!(
            result.is_ok(),
            expected,
            "{id} {value} {direction:?}: {result:?}"
        );
        if !expected {
            assert!(matches!(result, Err(Error::UnsupportedText { .. })));
        }
    }
    Ok(())
}
#[test]
fn hl7_251_2_7_1_2_7_6_law_text_kinds_have_distinct_escape_rules() -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        for code in ["F", "S", "R", "T", "E"] {
            text_cases(&format!("a\\{code}\\b"), direction, true, true, true)?;
        }
        for code in ["H", "N", "X41", "Zlocal"] {
            text_cases(&format!("a\\{code}\\b"), direction, false, true, true)?;
        }
        for code in [".sp", ".br", ".fi", ".nf", ".in+2", ".ti-2", ".sk2", ".ce"] {
            let authorable =
                direction == ControlDirection::Receive || !matches!(code, ".in+2" | ".ti-2");
            text_cases(
                &format!("a\\{code}\\b"),
                direction,
                false,
                false,
                authorable,
            )?;
        }
        for code in ["C2842", "M2442", "M242842"] {
            text_cases(&format!("a\\{code}\\b"), direction, false, false, false)?;
        }
    }
    Ok(())
}
#[test]
fn hl7_251_2_7_6_law_c5_ft_line_markers_preserve_layout_evidence() -> TestResult {
    for value in [
        "first^second",
        "first^\\.sp\\second",
        "\\.ti+2\\first^second",
        "\\.nf\\first^second",
        "first\\S\\second",
    ] {
        assert!(
            validate("NTE", &[(3, value)], ControlDirection::Receive)?.is_ok(),
            "{value}"
        );
    }
    for kind in ["ST", "TX"] {
        assert!(
            validate(
                "OBX",
                &[(2, kind), (5, "first^second")],
                ControlDirection::Receive
            )?
            .is_err()
        );
    }
    assert!(validate("NTE", &[(3, "first~second")], ControlDirection::Receive)?.is_err());
    // Preserve mode cannot bypass the LAW leaf boundary.
    let message = fixture("NTE", &[(3, "first^second")], ControlDirection::Receive)?;
    for invalid in [
        r"first^\.unknown\second",
        r"first^\.br 1\second",
        r"first^\X1\second",
        r"first^\H",
    ] {
        let raw = std::str::from_utf8(message.raw())?.rewrite("first^second", invalid);
        let preserved = Message::parse(
            raw.as_bytes(),
            Options {
                charset: Charset::Utf8,
                escapes: analyte_hl7::EscapePolicy::Preserve,
                ..Options::default()
            },
        )?;
        assert!(
            Profile::receive()
                .validate_segment(
                    preserved.segments().nth(1).ok_or("segment")?,
                    ControlDirection::Receive
                )
                .is_err()
        );
        assert_eq!(preserved.raw(), raw.as_bytes());
    }
    assert!(validate("PID", &[(4, r"ignored\H\")], ControlDirection::Receive)?.is_ok());

    Ok(())
}
