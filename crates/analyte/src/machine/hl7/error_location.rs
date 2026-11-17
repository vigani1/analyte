//! Locates HL7 and LAW errors without inventing unknown source positions.
use analyte_hl7::Terminator;
pub(crate) use analyte_hl7::location_error as build;
use analyte_mapping::Output;

pub(crate) use analyte_hl7::Location;
use analyte_hl7::syntax_location;

pub(crate) fn mapping_location(
    mapping: &Result<Output, crate::machine::interpretation::ReadError>,
    raw: &[u8],
    ending: Terminator,
) -> Option<Location> {
    let position = match mapping {
        Ok(output) => output.findings.first()?.position,
        Err(crate::machine::interpretation::ReadError::Mapping(error)) => {
            use crate::reading::Error as Read;
            let error = match error {
                Read::Hl7(error) => return syntax_location(error, raw, ending),
                Read::Mapping(error) => error,
                Read::LawMapping(error) => match error {
                    analyte_mapping::law::Error::Mapping(error) => error,
                    analyte_mapping::law::Error::MissingHeader
                    | analyte_mapping::law::Error::NoticeOrder
                    | analyte_mapping::law::Error::NoticePosition => return None,
                },
                Read::Astm(_)
                | Read::Law(_)
                | Read::Poct(_)
                | Read::Vidas(_)
                | Read::Autolyser(_)
                | Read::AstmFrame(_)
                | Read::AstmReceipt(_)
                | Read::MllpFrame(_)
                | Read::MllpReceipt(_)
                | Read::VidasFrame(_)
                | Read::AutolyserFrame(_)
                | Read::Encoding(_)
                | Read::Evidence(_)
                | Read::Context(_) => return None,
            };
            match error {
                analyte_mapping::Error::Limit { position, .. }
                | analyte_mapping::Error::Structure { position, .. }
                | analyte_mapping::Error::Encoding { position }
                | analyte_mapping::Error::Text { position, .. } => *position,
                analyte_mapping::Error::Hl7 { source, .. } => {
                    return syntax_location(source, raw, ending);
                }
                analyte_mapping::Error::CharsetMismatch
                | analyte_mapping::Error::DepartureKind
                | analyte_mapping::Error::DepartureEvidence(_) => {
                    return None;
                }
            }
        }
        Err(
            crate::machine::interpretation::ReadError::Store(_)
            | crate::machine::interpretation::ReadError::Deadline(_),
        ) => return None,
    };
    Location::at(raw, ending, position.record, position.field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl7_251_2_15_5_2_negative_ack_keeps_wrapped_syntax_and_mapping_locations() {
        use crate::machine::interpretation::ReadError;
        use analyte_hl7::{AckCode, ControlId, Message, Options, Response, Responsibility};
        let raw = b"MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ORU^R01^ORU_R01|message1|P|2.5.1\rOBX|1|NM|SYN||12\r";
        for error in [
            crate::reading::Error::Hl7(analyte_hl7::Error::Escape {
                segment: 2,
                field: 5,
                offset: 0,
            }),
            crate::reading::Error::Mapping(analyte_mapping::Error::Limit {
                position: analyte_mapping::Position {
                    record: 2,
                    field: 5,
                },
                resource: "synthetic bound",
            }),
        ] {
            let mapping = Err(ReadError::Mapping(error));
            let location = mapping_location(&mapping, raw, Terminator::Cr);
            let ack = analyte_hl7::acknowledgment(
                &Message::parse(raw, Options::default()).unwrap(),
                Response {
                    control_id: ControlId::new(b"reply1").unwrap(),
                    timestamp: b"202611151200",
                },
                AckCode::Ae,
                Responsibility::Stored,
                Some(build(location.as_ref(), b"207", b"operator-review")),
                Options::default(),
            )
            .unwrap()
            .unwrap();
            assert!(
                String::from_utf8_lossy(ack.raw()).contains("ERR||OBX^1^5|207|E|operator-review")
            );
        }
    }

    #[test]
    fn hl7_251_2_15_5_2_location_requires_source_occurrence_and_keeps_numeric_segment_ids() {
        let raw = b"MSH|^~\\&|\rPV1|1\rOBX|1\rOBX|2\r";
        let precise = syntax_location(
            &analyte_hl7::Error::Escape {
                segment: 4,
                field: 5,
                offset: 0,
            },
            raw,
            Terminator::Cr,
        );
        let error = analyte_hl7::location_error(precise.as_ref(), b"102", b"synthetic");
        assert_eq!(
            (error.segment, error.occurrence, error.field),
            (b"OBX".as_slice(), 2, 5)
        );
        assert!(
            syntax_location(
                &analyte_hl7::Error::Control {
                    segment: "OBX",
                    field: 5
                },
                raw,
                Terminator::Cr
            )
            .is_none()
        );
        let precise = syntax_location(
            &analyte_hl7::Error::Control {
                segment: "PV1",
                field: 2,
            },
            raw,
            Terminator::Cr,
        );
        let error = analyte_hl7::location_error(precise.as_ref(), b"102", b"synthetic");
        assert_eq!(
            (error.segment, error.occurrence, error.field),
            (b"PV1".as_slice(), 1, 2)
        );
        assert!(Location::at(raw, Terminator::Cr, 99, 5).is_none());
        assert!(Location::at(raw, Terminator::Cr, 4, 0).is_none());
    }
}
