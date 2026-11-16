//! Tests rule8 cancel refusal requires cancellation and retained state.
use analyte_model::OrderState::{self, *};
use analyte_model::OrderTransition;

const STATES: [OrderState; 9] = [
    Requested,
    Sending,
    Sent,
    Accepted,
    Done,
    Rejected,
    CancelRequested,
    CancelConfirmed,
    Uncertain,
];

macro_rules! row {
    ($test:ident, $from:ident, [$($allowed:ident),*]) => {
        #[test]
        fn $test() {
            let allowed: &[OrderState] = &[$($allowed),*];
            for to in STATES {
                let result = $from.transition(OrderTransition::Advance(to));
                if allowed.contains(&to) { assert_eq!(result, Ok(to)); }
                else { assert_eq!(result,Err(analyte_model::TransitionError { from:$from,to }), "illegal transition {:?} -> {:?}",$from,to); }
            }
        }
    };
}

row!(
    rule8_requested_moves,
    Requested,
    [Sending, CancelRequested, Rejected]
);
row!(
    rule8_sending_moves,
    Sending,
    [Sent, Accepted, Rejected, Uncertain]
);
row!(
    rule8_sent_moves,
    Sent,
    [Accepted, Rejected, CancelRequested, Uncertain]
);
row!(
    rule8_accepted_moves,
    Accepted,
    [Done, Rejected, CancelRequested, Uncertain]
);
row!(rule8_done_is_terminal, Done, []);
row!(rule8_rejected_is_terminal, Rejected, []);
row!(
    rule8_cancel_requested_moves,
    CancelRequested,
    [CancelConfirmed, Done, Rejected, Uncertain]
);
row!(rule8_cancel_confirmed_is_terminal, CancelConfirmed, []);
row!(
    rule8_uncertain_never_sends_again,
    Uncertain,
    [Accepted, Done, Rejected, CancelRequested, CancelConfirmed]
);

// ADR 0003 rule 8: explicit refusal restores only the recorded Sent/Accepted state.
#[test]
fn rule8_cancel_refusal_requires_cancellation_and_retained_state() {
    for from in STATES {
        for previous in STATES {
            let result = from.transition(OrderTransition::CancelRefused(previous));
            if from == CancelRequested && matches!(previous, Sent | Accepted) {
                assert_eq!(result, Ok(previous));
            } else {
                assert_eq!(
                    result,
                    Err(analyte_model::TransitionError { from, to: previous })
                );
            }
        }
    }
}
