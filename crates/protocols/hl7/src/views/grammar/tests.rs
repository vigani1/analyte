//! Tests ORU and OUL grammar state transitions against chapter 7.
use super::*;
fn accepts(ids: &[&[u8]], oul: bool) -> bool {
    let mut state = State::BeforeOrder(Patient::Absent);
    let mut previous = b"MSH".as_slice();
    for (i, id) in ids.iter().enumerate() {
        let Some(next) = state.step(id, previous, i.saturating_add(1), oul) else {
            return false;
        };
        state = next;
        previous = id;
    }
    state.order() && !state.pending()
}
// HL7 2.3.1 §7.2.1
#[test]
fn oru_segment_order_states() {
    assert!(accepts(
        &[b"PID", b"PV1", b"PV2", b"ORC", b"OBR", b"OBX", b"NTE"],
        false
    ));
    assert!(accepts(&[b"OBR", b"PID", b"OBR"], false));
    assert!(!accepts(&[b"ORC"], false));
    assert!(!accepts(&[b"ORC", b"PID", b"OBR"], false));
    assert!(!accepts(&[b"OBX", b"OBR"], false));
    assert!(!accepts(&[b"PID", b"PV2", b"OBR"], false));
    assert!(!accepts(&[b"PID", b"PV1", b"PV1", b"OBR"], false));
    assert!(!accepts(&[b"OBR", b"ORC"], false));
}
// HL7 2.5.1 §7.3.7
#[test]
fn oul_segment_order_states() {
    assert!(accepts(
        &[
            b"NTE", b"PID", b"PV1", b"SPM", b"SAC", b"OBX", b"OBR", b"ORC", b"OBX", b"TCD", b"SID"
        ],
        true
    ));
    assert!(accepts(&[b"SPM", b"OBR", b"SPM", b"OBR"], true));
    assert!(!accepts(&[b"OBR"], true));
    assert!(!accepts(&[b"SPM"], true));
    assert!(!accepts(&[b"SPM", b"SPM", b"OBR"], true));
    assert!(!accepts(&[b"SPM", b"OBR", b"ORC", b"ORC"], true));
    assert!(!accepts(&[b"SPM", b"OBR", b"SAC"], true));
    assert!(!accepts(&[b"SPM", b"OBR", b"OBX", b"ORC"], true));
}
