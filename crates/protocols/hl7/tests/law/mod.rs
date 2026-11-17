//! Tests LAW message validation and host exchanges.
use analyte_hl7::{
    Message, Options,
    law::{Error, Query, Scope},
};
use proptest::prelude::*;

fn query(body: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101120000+0000||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r").as_bytes(), utf8_options())
}

fn response_metadata() -> Result<analyte_hl7::Response<'static>, analyte_hl7::Error> {
    Ok(analyte_hl7::Response {
        control_id: analyte_hl7::ControlId::new(b"rsp-1")?,
        timestamp: b"20260101120000+0000",
    })
}

fn address() -> analyte_hl7::law::Address<'static> {
    analyte_hl7::law::Address {
        application: b"M",
        facility: b"MF",
        peer_application: b"A",
        peer_facility: b"AF",
    }
}

fn utf8_options() -> Options {
    Options {
        charset: analyte_hl7::Charset::Utf8,
        ..Options::default()
    }
}

fn results_message(awos: &str, extra: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|result-1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||000tube\rOBR||{awos}||test^Test^99LAB\rORC|SC||||CM\r{extra}").as_bytes(), utf8_options())
}

fn observation(code: &str, sub_id: &str, kind: &str, value: &str, status: &str) -> String {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, kind),
        (2, code),
        (3, sub_id),
        (4, value),
        (
            5,
            if matches!(kind, "NM" | "SN") {
                "mg/L^mg/L^UCUM"
            } else {
                ""
            },
        ),
        (6, "0-2"),
        (7, "N^Normal^HL70078"),
        (10, status),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!("OBX|{}\r", fields.join("|"))
}

fn work_capabilities() -> analyte_hl7::law::Capabilities {
    analyte_hl7::law::Capabilities {
        content: analyte_hl7::law::PeerContent {
            specimen: true,
            ..analyte_hl7::law::Capabilities::default().content
        },
        ..analyte_hl7::law::Capabilities::default()
    }
}

fn validate_profile_body(body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let message = query(body)?;
    let segment = message
        .segments()
        .nth(1)
        .ok_or("missing synthetic segment")?;
    analyte_hl7::law::validate_segment(segment)?;
    Ok(())
}

mod containers;

mod unsolicited;

mod locations;
