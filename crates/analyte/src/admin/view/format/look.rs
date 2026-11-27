//! Gives each order, delivery and attempt state one tone, icon and word, used by every page that shows it.
use crate::admin::view::{Tone, icon::Icon};
use analyte_i18n::text as tr;
use analyte_model::OrderState;
use analyte_store::{AttemptOutcome, DeliveryState};

/// What a machine reported doing with an order item, kept apart from the
/// item's own state.
pub(in crate::admin) fn execution(state: analyte_model::AwosExecution) -> &'static str {
    match state {
        analyte_model::AwosExecution::Scheduled => tr("admin-scheduled-on-the-machine"),
        analyte_model::AwosExecution::InProgress => tr("admin-in-progress-on-the-machine"),
        analyte_model::AwosExecution::AnalyzerCancelled => tr("admin-cancelled-by-the-machine"),
    }
}

/// How a delivery or order state reads on every page that shows it: its
/// tone, its own icon and its word. One rule picks the tone: a problem (red)
/// when someone must act, attention (amber) for a look or a retry, neutral
/// (grey) while waiting or when nothing is wrong, good (green) when done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) struct StateLook {
    pub(in crate::admin) tone: Tone,
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) word: &'static str,
}

impl StateLook {
    const fn of(tone: Tone, icon: Icon, word: &'static str) -> Self {
        Self { tone, icon, word }
    }
}

/// An order's state as every page shows it; pages keep the stored state on
/// hover. Sent, accepted, done and cancelled stay separate (safety rule 8).
pub(in crate::admin) fn order_look(state: OrderState) -> StateLook {
    match state {
        OrderState::Requested => {
            StateLook::of(Tone::Neutral, Icon::Waiting, tr("admin-not-sent-yet"))
        }
        OrderState::Sending => {
            StateLook::of(Tone::Neutral, Icon::Send, tr("doctor-overview-sending"))
        }
        // Owner decision 2026-11-20: sent and not yet accepted is waiting.
        OrderState::Sent => StateLook::of(
            Tone::Neutral,
            Icon::Waiting,
            tr("admin-sent-not-yet-accepted"),
        ),
        OrderState::Accepted => {
            StateLook::of(Tone::Good, Icon::Check, tr("admin-accepted-by-the-machine"))
        }
        OrderState::Done => StateLook::of(Tone::Good, Icon::Ok, tr("setup-done")),
        OrderState::Rejected => StateLook::of(
            Tone::Problem,
            Icon::Problem,
            tr("admin-rejected-by-the-machine"),
        ),
        OrderState::CancelRequested => {
            StateLook::of(Tone::Neutral, Icon::Waiting, tr("admin-cancel-requested"))
        }
        OrderState::CancelConfirmed => {
            StateLook::of(Tone::Neutral, Icon::Cancelled, tr("admin-cancelled"))
        }
        OrderState::Uncertain => StateLook::of(
            Tone::Attention,
            Icon::Unknown,
            tr("admin-outcome-unknown-check-the-machine"),
        ),
    }
}

/// A delivery's state as every page shows it; pages keep the stored state on hover.
pub(in crate::admin) fn delivery_look(state: DeliveryState) -> StateLook {
    match state {
        DeliveryState::Pending => StateLook::of(
            Tone::Neutral,
            Icon::Waiting,
            crate::admin::view::format::waiting_to_send(),
        ),
        DeliveryState::InFlight => {
            StateLook::of(Tone::Neutral, Icon::Send, tr("doctor-overview-sending"))
        }
        DeliveryState::Done => StateLook::of(Tone::Good, Icon::Ok, tr("admin-delivered")),
        // Owner decision 2026-11-20: a rejected delivery needs someone.
        DeliveryState::Parked => StateLook::of(
            Tone::Problem,
            Icon::Problem,
            tr("admin-rejected-by-the-lab-app"),
        ),
        DeliveryState::Paused => StateLook::of(
            Tone::Problem,
            Icon::Problem,
            tr("admin-paused-lab-app-settings-need-repair"),
        ),
        DeliveryState::Conflict => StateLook::of(
            Tone::Problem,
            Icon::Problem,
            tr("admin-lab-app-reports-a-conflict"),
        ),
        // ADR 0038: a lab app may take no Needs review notices; nothing waits.
        DeliveryState::Declined => StateLook::of(
            Tone::Neutral,
            Icon::Ok,
            tr("admin-not-sent-the-lab-app-takes-no-notices"),
        ),
    }
}

/// How one try to send a delivery ended, as Activity shows it: the delivery
/// state it left, or a retry, which is not a state of its own.
pub(in crate::admin) fn attempt_look(outcome: AttemptOutcome) -> StateLook {
    match outcome {
        AttemptOutcome::Delivered => delivery_look(DeliveryState::Done),
        AttemptOutcome::RetryAt(_) | AttemptOutcome::Interrupted => {
            StateLook::of(Tone::Attention, Icon::Retry, tr("admin-will-retry"))
        }
        AttemptOutcome::Rejected => delivery_look(DeliveryState::Parked),
        AttemptOutcome::Paused => delivery_look(DeliveryState::Paused),
        AttemptOutcome::Conflict => delivery_look(DeliveryState::Conflict),
    }
}
