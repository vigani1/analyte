//! Tests AWOS execution independently of command and observation status.
use analyte_model::AwosExecution::{AnalyzerCancelled as C, InProgress as I, Scheduled as S};

#[test]
fn palm_tf2b_c8_figure_c8_1_adr0016_execution_transition_table() {
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
