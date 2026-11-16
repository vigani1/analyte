//! Writes stable HL7 acknowledgment phase evidence.
use super::AckPhase;

impl std::fmt::Display for AckPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Accept => "Accept",
            Self::Application => "Application",
        })
    }
}
#[test]
fn ack_phase_spellings_are_compatible() {
    assert_eq!(AckPhase::Accept.to_string(), "Accept");
    assert_eq!(AckPhase::Application.to_string(), "Application");
}
