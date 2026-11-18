//! Tests AWOS execution independently of command and observation status.
use analyte_model::AwosExecution::{AnalyzerCancelled as C, InProgress as I, Scheduled as S};

// IHE PaLM TF-2b §C.8, Figure C.8-1; ADR 0016
#[test]
fn execution_transitions_follow_the_table() {
    for (previous, allowed) in [
        (S, [true, true, true]),
        (I, [false, true, false]),
        (C, [false, false, true]),
    ] {
        for (incoming, expected) in [S, I, C].into_iter().zip(allowed) {
            assert_eq!(
                previous.advance(incoming).is_ok(),
                expected,
                "{previous:?} -> {incoming:?}"
            );
        }
    }
}
