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
        Err(error) => {
            let error: &(dyn std::error::Error + 'static) = error;
            if let Some(error) = error.downcast_ref::<analyte_hl7::Error>() {
                return syntax_location(error, raw, ending);
            }
            match error.downcast_ref::<analyte_mapping::Error>()? {
                analyte_mapping::Error::Limit { position, .. }
                | analyte_mapping::Error::Structure { position, .. }
                | analyte_mapping::Error::Encoding { position }
                | analyte_mapping::Error::Text { position, .. } => *position,
                // Control wrappers do not always identify the same field as their cause.
                analyte_mapping::Error::Hl7 { source, .. } => {
                    return syntax_location(source, raw, ending);
                }
                analyte_mapping::Error::Syntax | analyte_mapping::Error::DepartureEvidence(_) => {
                    return None;
                }
            }
        }
    };
    Location::at(raw, ending, position.record, position.field)
}

#[cfg(test)]
mod tests {
    use super::*;

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
