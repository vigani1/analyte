//! Locates HL7 and LAW errors without inventing unknown source positions, and
//! tells a syntax failure apart from a held interpretation.
use analyte_hl7::Terminator;
use analyte_mapping::Output;

pub(crate) use analyte_hl7::Location;
use analyte_hl7::syntax_location;

/// What the HL7 acknowledgment needs to know about a stored message's reading.
#[derive(Debug, Default)]
pub(crate) struct Diagnosis {
    /// Where reading stopped, when the source position is known.
    pub location: Option<Location>,
    /// Why an interpretation that was not accepted failed.
    pub cause: Cause,
}

/// Why a stored HL7 message was not accepted; only a held one still gets AA
/// (ADR 0010, 2026-11-26).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// Readable HL7 whose interpretation is held for an operator.
    #[default]
    Held,
    /// The stored bytes break HL7 syntax, a data type or framing.
    Syntax,
    /// MSH-9 names a transaction the result grammar does not read.
    Unsupported,
}

pub(crate) fn diagnose(
    mapping: &Result<Output, crate::machine::interpretation::ReadError>,
    raw: &[u8],
    ending: Terminator,
    unsupported: bool,
) -> Diagnosis {
    let cause = if syntax_failure(mapping) {
        Cause::Syntax
    } else if unsupported {
        Cause::Unsupported
    } else {
        Cause::Held
    };
    Diagnosis {
        location: mapping_location(mapping, raw, ending),
        cause,
    }
}

fn syntax_failure(mapping: &Result<Output, crate::machine::interpretation::ReadError>) -> bool {
    use crate::reading::Error as Read;
    let Err(crate::machine::interpretation::ReadError::Mapping(error)) = mapping else {
        return false;
    };
    match error {
        Read::Hl7(_)
        | Read::MllpFrame(_)
        | Read::MllpReceipt(_)
        | Read::Encoding(_)
        | Read::Mapping(analyte_mapping::Error::Hl7 { .. }) => true,
        Read::Mapping(
            analyte_mapping::Error::Limit { .. }
            | analyte_mapping::Error::Structure { .. }
            | analyte_mapping::Error::Encoding { .. }
            | analyte_mapping::Error::Text { .. }
            | analyte_mapping::Error::CharsetMismatch
            | analyte_mapping::Error::DepartureKind
            | analyte_mapping::Error::DepartureEvidence(_),
        )
        | Read::RecordedOnly
        | Read::LawMapping(_)
        | Read::Astm(_)
        | Read::Law(_)
        | Read::Poct(_)
        | Read::Vidas(_)
        | Read::Autolyser(_)
        | Read::AstmFrame(_)
        | Read::AstmReceipt(_)
        | Read::VidasFrame(_)
        | Read::AutolyserFrame(_)
        | Read::Evidence(_)
        | Read::Context(_) => false,
    }
}

fn mapping_location(
    mapping: &Result<Output, crate::machine::interpretation::ReadError>,
    raw: &[u8],
    ending: Terminator,
) -> Option<Location> {
    let position = match mapping {
        Ok(output) => output.findings.first()?.position,
        Err(crate::machine::interpretation::ReadError::Mapping(error)) => {
            let error = match error.hl7_mapping_error() {
                Some(Ok(source)) => return syntax_location(source, raw, ending),
                Some(Err(error)) => error,
                None => return None,
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

    // HL7 2.5.1 §2.15.5.2
    #[test]
    fn negative_ack_keeps_syntax_and_mapping_locations() {
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
                Some(analyte_hl7::location_error(
                    location.as_ref(),
                    analyte_hl7::APPLICATION_INTERNAL_ERROR,
                    b"operator-review",
                )),
                Options::default(),
            )
            .unwrap()
            .unwrap();
            assert!(
                String::from_utf8_lossy(ack.raw()).contains("ERR||OBX^1^5|207|E|operator-review")
            );
        }
    }

    // HL7 2.5.1 §2.9.2; ADR 0010 (2026-11-26): an HL7 syntax failure or an
    // unsupported transaction keeps AE; a mapping problem such as a missing
    // OBX-3 is a held interpretation.
    #[test]
    fn only_syntax_and_unsupported_transactions_keep_ae() {
        use crate::machine::interpretation::ReadError;
        let raw = b"MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||ORU^R01^ORU_R01|message1|P|2.5.1\rOBX|1|NM|||12\r";
        let position = analyte_mapping::Position {
            record: 2,
            field: 3,
        };
        let cases: [(Result<Output, ReadError>, Cause); 6] = [
            (Ok(Output::default()), Cause::Held),
            (
                Err(ReadError::Mapping(crate::reading::Error::Hl7(
                    analyte_hl7::Error::Escape {
                        segment: 2,
                        field: 5,
                        offset: 0,
                    },
                ))),
                Cause::Syntax,
            ),
            (
                Err(ReadError::Mapping(crate::reading::Error::Mapping(
                    analyte_mapping::Error::Hl7 {
                        position,
                        source: analyte_hl7::Error::Control {
                            segment: "OBX",
                            field: 14,
                        },
                    },
                ))),
                Cause::Syntax,
            ),
            (
                Err(ReadError::Mapping(crate::reading::Error::Mapping(
                    analyte_mapping::Error::Structure {
                        position,
                        reason: "synthetic missing observation identifier",
                    },
                ))),
                Cause::Held,
            ),
            (
                Err(ReadError::Mapping(crate::reading::Error::Mapping(
                    analyte_mapping::Error::Limit {
                        position,
                        resource: "synthetic bound",
                    },
                ))),
                Cause::Held,
            ),
            (
                Err(ReadError::Mapping(crate::reading::Error::Mapping(
                    analyte_mapping::Error::CharsetMismatch,
                ))),
                Cause::Held,
            ),
        ];
        for (mapping, cause) in cases {
            assert_eq!(diagnose(&mapping, raw, Terminator::Cr, false).cause, cause);
        }
        let unsupported = Err(ReadError::Mapping(crate::reading::Error::Mapping(
            analyte_mapping::Error::Structure {
                position: analyte_mapping::Position {
                    record: 1,
                    field: 9,
                },
                reason: "synthetic unsupported transaction",
            },
        )));
        assert_eq!(
            diagnose(&unsupported, raw, Terminator::Cr, true).cause,
            Cause::Unsupported
        );
    }

    // HL7 2.5.1 §2.15.5.2
    #[test]
    fn location_needs_occurrence_and_keeps_numeric_segment_ids() {
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
