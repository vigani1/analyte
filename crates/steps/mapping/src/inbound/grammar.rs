//! Checks the selected result grammar before constructing group ownership.
use crate::{Error, Position};

// HL7 2.3.1 section 7.2.1; 2.5.1 section 7.3.1 and 7.3.7.
pub(super) fn validate(message: &analyte_hl7::Message, oul: bool) -> Result<(), Error> {
    let mut patient = false;
    let mut visit = false;
    let mut visit_extra = false;
    let mut order = false;
    let mut specimen = false;
    let mut pending_control = false;
    let mut oul_control = false;
    for (index, segment) in message.segments().enumerate().skip(1) {
        let valid = match segment.id() {
            b"PID" => {
                let valid =
                    !pending_control && (!patient || (!oul && order)) && (!oul || !specimen);
                patient = true;
                visit = false;
                visit_extra = false;
                order = false;
                specimen = false;
                valid
            }
            b"PV1" => {
                let valid = !visit && (oul || patient) && !order && !specimen;
                visit = true;
                valid
            }
            b"PV2" => {
                let valid = visit && !visit_extra && !order && !specimen;
                visit_extra = true;
                valid
            }
            b"ORC" if oul => {
                let valid = order && !oul_control;
                oul_control = true;
                valid
            }
            b"ORC" => {
                let valid = !pending_control;
                pending_control = true;
                order = false;
                valid
            }
            b"OBR" => {
                let valid = !oul || specimen;
                order = true;
                pending_control = false;
                oul_control = false;
                valid
            }
            b"SPM" if oul => {
                let valid = !specimen || order;
                specimen = true;
                order = false;
                oul_control = false;
                valid
            }
            b"SPM" => order && !pending_control,
            b"SAC" => oul && specimen && !order,
            b"OBX" => !pending_control && (order || (oul && specimen)),
            b"NTE" => !pending_control,
            // Ownership validation below checks the narrow OUL result-detail positions.
            b"TCD" | b"SID" => oul && order,
            _ => false,
        };
        if !valid {
            return Err(Error::Structure {
                position: Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                reason: "segment violates selected HL7 result grammar; retain receipt for review",
            });
        }
    }
    if pending_control || !order {
        return Err(Error::Structure {
            position: Position {
                record: message.segments().count(),
                field: 0,
            },
            reason: "incomplete HL7 result group; retain receipt for review",
        });
    }
    Ok(())
}
