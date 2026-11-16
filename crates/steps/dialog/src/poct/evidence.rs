//! Writes stable POCT custody-ticket evidence independently of derived Debug output.
use super::CommitId;
use std::fmt::{self, Write};

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CommitId { session: ControlId(\"")?;
        for character in self.session.as_str().chars() {
            if character == '\'' {
                f.write_char(character)?;
            } else {
                for escaped in character.escape_debug() {
                    f.write_char(escaped)?;
                }
            }
        }
        write!(f, "\"), sequence: {} }}", self.sequence)
    }
}

/// Returns the historical protected-store list spelling for unresolved custody tickets.
#[must_use]
pub fn ticket_evidence(tickets: &[CommitId]) -> String {
    format!(
        "[{}]",
        tickets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[test]
fn custody_ticket_spellings_are_compatible() -> Result<(), analyte_poct::MessageError> {
    assert_eq!(ticket_evidence(&[]), "[]");
    let tickets = [
        CommitId {
            session: analyte_poct::ControlId::new("S-1")?,
            sequence: 1,
        },
        CommitId {
            session: analyte_poct::ControlId::new("a'\"?")?,
            sequence: u64::MAX,
        },
    ];
    assert_eq!(
        ticket_evidence(&tickets),
        "[CommitId { session: ControlId(\"S-1\"), sequence: 1 }, CommitId { session: ControlId(\"a'\\\"?\"), sequence: 18446744073709551615 }]"
    );
    Ok(())
}
