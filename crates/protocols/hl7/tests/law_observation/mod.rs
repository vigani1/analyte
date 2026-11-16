//! Original synthetic cases: `PaLM` TF-2b C.2/C.5/C.7, section 2.2 and HL7 2.5.1 2A.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection, Error, Profile, ResultStatus},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const TEST: &str = "test^Synthetic^99LAB";

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}

fn fields(
    id: &str,
    direction: ControlDirection,
    changes: &[(usize, &str)],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = [""; 40];
    let defaults = match id {
        "OBX" => vec![
            (1, "1"),
            (2, "NM"),
            (3, TEST),
            (4, "1"),
            (5, "1.20"),
            (6, "mg/L^original unit^UCUM"),
            (8, "N^^HL70078"),
            (11, "F"),
            (16, "operator"),
            (18, "model^maker~serial^maker"),
            (19, "20260101120000"),
            (29, "RSLT"),
        ],
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
            (3, "Synthetic comment"),
        ],
        "INV" => vec![
            (1, "substance^^99LAB"),
            (2, "NA^^HL70383"),
            (3, "MR^^HL70384"),
        ],
        _ => return Err("fixture segment".into()),
    };
    for (pos, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(pos).ok_or("fixture field")? = value;
    }
    Ok(format!(
        "{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    ))
}

fn message(body: &str) -> Result<Message, analyte_hl7::Error> {
    let wire = format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\r{body}"
    );
    law::parse_message(wire.as_bytes(), options())
}

fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(&fields(id, direction, changes)?)?;
    let original = message.encode();
    let result = profile.validate_segment(message.segments().nth(1).ok_or("fixture")?, direction);
    assert_eq!(message.raw(), original);
    Ok(result?)
}

fn both(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert_eq!(
            check(id, changes, Profile::receive(), direction).is_ok(),
            valid,
            "{id} {changes:?} {direction:?}"
        );
    }
    Ok(())
}

fn result_message(
    changes: &[(usize, &str)],
    suffix: &str,
) -> Result<Message, Box<dyn std::error::Error>> {
    message(&format!("SPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||{TEST}\rORC|SC||||CM\r{}{suffix}",fields("OBX",ControlDirection::Receive,changes)?)).map_err(Into::into)
}

mod scenarios_1;

mod scenarios_2;

mod scenarios_3;
