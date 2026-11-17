//! Tests HL7 order messages and acknowledgments.
#[macro_use]
mod assertions;
use analyte_hl7::order::{
    self, Code, Control, Error, Header, Order, OrderId, OrderMessage, OrderResponse, Outcome,
    Patient, Priority, Processing, SampleField, SampleId, Specimen, Subject, Timestamp, Timing,
};
use analyte_hl7::{AckCode, ControlId, Message, Options, Version, decode_text};
mod commands;
mod responses;

fn header(version: Version) -> Result<Header<'static>, Error> {
    Ok(Header {
        application: b"HOST",
        facility: b"LAB",
        peer_application: b"MACHINE",
        peer_facility: b"LAB",
        control_id: ControlId::new(b"sent1")?,
        timestamp: Timestamp::new(b"20261115090000+0100")?,
        version,
        processing: Processing::Training,
    })
}

fn new_order(sample: &[u8], field: SampleField) -> Result<Order<'_>, Error> {
    Ok(Order::New {
        placer: OrderId::new(b"001", b"LAB")?,
        filler: None,
        test: Code::new(b"T1", b"Synthetic", b"L")?,
        specimen: Specimen {
            sample: SampleId::new(sample)?,
            field,
            source: Some(Code::new(b"SER", b"Serum", b"L")?),
        },
        priority: Priority::Stat,
        timing: Timing {
            priority_version: None,
            start: Some(Timestamp::new(b"202611151000")?),
            end: Some(Timestamp::new(b"202611151100")?),
        },
    })
}

fn sent(version: Version) -> Result<OrderMessage, Error> {
    order::orm(
        &[
            new_order(b"000aB", SampleField::Placer1)?,
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: Some(OrderId::new(b"F2", b"M")?),
            },
        ],
        Subject::Patient(Patient {
            identifier_type: None,
            identifier: b"P1",
            namespace: b"LAB",
            family_name: b"Synthetic",
        }),
        header(version)?,
        Options::default(),
    )
}

fn reply(version: Version, code: &[u8], body: &[u8]) -> Result<Message, Error> {
    let edition = match version {
        Version::V23
        | Version::V24
        | Version::V25
        | Version::V26
        | Version::V282
        | Version::V291 => unreachable!("fixture enumerates only 2.3.1 and 2.5.1"),
        Version::V231 => "2.3.1",
        Version::V251 => "2.5.1",
    };
    let mut bytes = format!(
        "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611150901||ORR^O02^ORR_O02|reply1|T|{edition}\rMSA|"
    )
    .into_bytes();
    bytes.extend_from_slice(code);
    bytes.extend_from_slice(b"|sent1\r");
    bytes.extend_from_slice(body);
    Ok(Message::parse(&bytes, Options::default())?)
}

fn orc(control: &[u8], placer: &[u8], filler: &[u8], status: &[u8], reason: &[u8]) -> Vec<u8> {
    let mut bytes = b"ORC".to_vec();
    for value in [
        control, placer, filler, b"", status, b"", b"", b"", b"", b"", b"", b"", b"", b"", b"",
        reason,
    ] {
        bytes.push(b'|');
        bytes.extend_from_slice(value);
    }
    bytes.push(b'\r');
    bytes
}
