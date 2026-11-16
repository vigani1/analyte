//! Tests LAW MSH, MSA and ERR validation with synthetic messages (TF-2b C.1/C.3/C.4 and 2.2.1?2.2.3; HL7 2.5.1 ch.2A).
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}

fn wire(
    kind: &str,
    changes: &[(usize, &str)],
    body: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let (message_type, profile, accept, application) = match kind {
        "query" => ("QBP^Q11^QBP_Q11", "LAB-27^IHE", "NE", "AL"),
        "work" => ("OML^O33^OML_O33", "LAB-28^IHE", "NE", "AL"),
        "results" => ("OUL^R22^OUL_R22", "LAB-29^IHE", "NE", "AL"),
        "rsp" => ("RSP^K11^RSP_K11", "LAB-27^IHE", "", ""),
        "orl" => ("ORL^O34^ORL_O42", "LAB-28^IHE", "", ""),
        "ack" => ("ACK^R22^ACK", "LAB-29^IHE", "", ""),
        _ => return Err("unknown synthetic message kind".into()),
    };
    let mut fields = vec![""; 25];
    for (number, value) in [
        (2, "^~\\&"),
        (3, "Analyzer"),
        (4, "Lab"),
        (5, "Manager"),
        (6, "Lab"),
        (7, "20260228123059+0130"),
        (9, message_type),
        (10, "000Ab"),
        (11, "P"),
        (12, "2.5.1"),
        (15, accept),
        (16, application),
        (18, "UNICODE UTF-8"),
        (21, profile),
    ]
    .into_iter()
    .chain(changes.iter().copied())
    {
        *fields
            .get_mut(number)
            .ok_or("synthetic field out of range")? = value;
    }
    Ok(format!(
        "MSH|{}\r{body}",
        fields.get(2..).ok_or("synthetic header")?.join("|")
    ))
}

fn check(
    kind: &str,
    changes: &[(usize, &str)],
    body: &str,
    direction: ControlDirection,
) -> Result<Message, Box<dyn std::error::Error>> {
    let bytes = wire(kind, changes, body)?;
    let parsed = law::parse_message(bytes.as_bytes(), options())?;
    law::validate_control(&parsed, direction)?;
    assert_eq!(parsed.raw(), bytes.as_bytes());
    Ok(parsed)
}

fn both(kind: &str, changes: &[(usize, &str)], body: &str, valid: bool) -> TestResult {
    for direction in [ControlDirection::Receive, ControlDirection::Send] {
        assert_eq!(
            check(kind, changes, body, direction).is_ok(),
            valid,
            "kind={kind}, changes={changes:?}, direction={direction:?}, body={body}"
        );
    }
    Ok(())
}

fn error_body(field: usize, value: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = vec![""; 13];
    *fields.get_mut(2).ok_or("synthetic location")? = "OBX^1^5^1^1^1";
    *fields.get_mut(3).ok_or("synthetic code")? = "100^^HL70357";
    *fields.get_mut(4).ok_or("synthetic severity")? = "E";
    *fields.get_mut(field).ok_or("synthetic field")? = value;
    Ok(format!(
        "MSA|AE|000Ab\rERR|{}\r",
        fields.get(1..).ok_or("synthetic ERR")?.join("|")
    ))
}

fn response<'a>(id: &'a [u8]) -> Result<analyte_hl7::Response<'a>, analyte_hl7::Error> {
    Ok(analyte_hl7::Response {
        control_id: law::control_id(id)?,
        timestamp: b"20260101123000+0000",
    })
}

fn query_with(changes: &[(usize, &str)]) -> Result<Message, Box<dyn std::error::Error>> {
    let body = "QPD|WOS^Synthetic^IHELAW|tag|000tube\rRCP|I||R^Real Time^HL70394\r";
    let bytes = wire("query", changes, body)?;
    Ok(law::parse_message(bytes.as_bytes(), options())?)
}

mod scenarios_1;

mod scenarios_2;
