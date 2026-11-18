//! Tests allowed and forbidden order lifecycle transitions.
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

// Safety rule 8 (ADR 0003)
row!(
    requested_state_moves,
    Requested,
    [Sending, CancelRequested, Rejected]
);
// Safety rule 8 (ADR 0003)
row!(
    sending_state_moves,
    Sending,
    [Sent, Accepted, Rejected, Uncertain]
);
// Safety rule 8 (ADR 0003)
row!(
    sent_state_moves,
    Sent,
    [Accepted, Rejected, CancelRequested, Uncertain]
);
// Safety rule 8 (ADR 0003)
row!(
    accepted_state_moves,
    Accepted,
    [Done, Rejected, CancelRequested, Uncertain]
);
// Safety rule 8 (ADR 0003)
row!(done_state_is_terminal, Done, []);
// Safety rule 8 (ADR 0003)
row!(rejected_state_is_terminal, Rejected, []);
// Safety rule 8 (ADR 0003)
row!(
    cancel_requested_state_moves,
    CancelRequested,
    [CancelConfirmed, Done, Rejected, Uncertain]
);
// Safety rule 8 (ADR 0003)
row!(cancel_confirmed_state_is_terminal, CancelConfirmed, []);
// Safety rule 8 (ADR 0003)
row!(
    uncertain_state_never_sends_again,
    Uncertain,
    [Accepted, Done, Rejected, CancelRequested, CancelConfirmed]
);

// ADR 0003 rule 8: explicit refusal restores only the recorded Sent/Accepted state.
// Safety rule 8 (ADR 0003)
#[test]
fn cancel_refusal_restores_only_retained_state() {
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
