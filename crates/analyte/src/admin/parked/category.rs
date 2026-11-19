//! Holds what each kind of Needs review item means for people: its words, its tone, the action that settles it and what to do.
//!
//! The guidance says only what the category and the problem read back from the
//! stored reason say; a mapping finding names the problem, never the test code,
//! which stays in the protected original message.
use crate::admin::view::Tone;
use analyte_mapping::{Finding, Problem};
use analyte_store::{Parked, ParkedKind};

/// The audited action that settles an item from the admin pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// Read the original message again with the current settings.
    ProcessAgain,
    /// Queue the rejected delivery again, unchanged.
    SendAgain,
    /// Mark the item resolved after someone checked it; it stays in the history.
    Resolve,
}

/// One item's category as the pages show it.
pub(super) struct Category {
    /// The category in plain words, or the stored kind when this version does not know it.
    pub(super) shown: String,
    /// The stored kind, on hover; empty when it already shows as stored.
    pub(super) stored: String,
    pub(super) tone: Tone,
    /// The action that exists today for this category.
    pub(super) action: Action,
    /// What someone does about the item.
    pub(super) guidance: &'static str,
}

impl Category {
    /// Reads the category of `parked` and, for a mapping finding, the problem
    /// in its stored reason. An unknown category keeps the Process again it
    /// always had.
    pub(super) fn of(parked: &Parked) -> Self {
        let stored = parked.kind.as_str();
        let Some(kind) = ParkedKind::from_stored(stored) else {
            return Self {
                shown: stored.to_owned(),
                stored: String::new(),
                tone: Tone::Attention,
                action: Action::ProcessAgain,
                guidance: concat!(
                    "Read the reason above and open the item for its linked records. If it is ",
                    "not clear what to do, give the item number to support.",
                ),
            };
        };
        let finding = Finding::from_stored(parked.reason.as_str());
        Self {
            shown: words(kind).to_owned(),
            stored: stored.to_owned(),
            tone: tone(kind),
            action: main_action(kind),
            guidance: guidance(kind, finding.as_ref().map(|f| &f.problem)),
        }
    }
}

/// The category in plain words.
const fn words(kind: ParkedKind) -> &'static str {
    match kind {
        ParkedKind::MappingFinding => "Test or unit not mapped",
        ParkedKind::InterpretationFailure => "Could not read message",
        ParkedKind::ResultReconciliation => "Result could not be matched to earlier results",
        ParkedKind::IncompleteTransfer => "Message from the machine was incomplete",
        ParkedKind::DeliveryRejected => "Lab app rejected a delivery",
        ParkedKind::DeliveryConflict => "Lab app reported a conflicting delivery",
        ParkedKind::OrderVersionRefused => "Order from the lab app was refused",
        ParkedKind::CancelConflict => "Cancel conflicts with what the machine reported",
        ParkedKind::PollIntent => "Machine asked for orders; the outcome is not known yet",
    }
}

/// How an open item of this category is marked: red when the lab app and
/// Analyte, or the lab app and the machine, disagree about what was sent;
/// amber for everything else that waits for someone.
const fn tone(kind: ParkedKind) -> Tone {
    match kind {
        ParkedKind::DeliveryRejected
        | ParkedKind::DeliveryConflict
        | ParkedKind::CancelConflict => Tone::Problem,
        ParkedKind::MappingFinding
        | ParkedKind::InterpretationFailure
        | ParkedKind::ResultReconciliation
        | ParkedKind::IncompleteTransfer
        | ParkedKind::OrderVersionRefused
        | ParkedKind::PollIntent => Tone::Attention,
    }
}

/// The action that exists today for this category. Process again reads the
/// item's original message, so only items linked to a message offer it. A
/// conflicting delivery is never offered Send again: the lab app already holds
/// other content under the same delivery.
const fn main_action(kind: ParkedKind) -> Action {
    match kind {
        ParkedKind::MappingFinding
        | ParkedKind::InterpretationFailure
        | ParkedKind::ResultReconciliation
        | ParkedKind::IncompleteTransfer => Action::ProcessAgain,
        ParkedKind::DeliveryRejected => Action::SendAgain,
        ParkedKind::DeliveryConflict
        | ParkedKind::OrderVersionRefused
        | ParkedKind::CancelConflict
        | ParkedKind::PollIntent => Action::Resolve,
    }
}

/// What someone does about the item.
const fn guidance(kind: ParkedKind, problem: Option<&Problem>) -> &'static str {
    match kind {
        ParkedKind::MappingFinding if matches!(problem, Some(Problem::UnknownTest)) => concat!(
            "The machine sent a test code that this machine's test mapping does not ",
            "list. Add it to the test mapping, then use Process again.",
        ),
        ParkedKind::MappingFinding
            if matches!(problem, Some(Problem::ValueKindMismatch { .. })) =>
        {
            concat!(
                "The value does not fit what the test mapping expects for this test. ",
                "Check that test in this machine's test mapping, then use Process again.",
            )
        }
        ParkedKind::MappingFinding if matches!(problem, Some(Problem::MissingSample)) => concat!(
            "The message has no sample ID. Check how the machine sends sample IDs; once ",
            "its settings are fixed, use Process again.",
        ),
        ParkedKind::MappingFinding => concat!(
            "Check this machine's test mapping and settings against the reason above, ",
            "then use Process again.",
        ),
        ParkedKind::InterpretationFailure => concat!(
            "Check the machine's settings on the Setup page (machine model and ",
            "connection) against the reason above, then use Process again.",
        ),
        ParkedKind::ResultReconciliation => concat!(
            "Compare this result with the earlier results for the same sample on the ",
            "machine. If a setting caused the mismatch, fix it, then use Process again.",
        ),
        ParkedKind::IncompleteTransfer => concat!(
            "The machine stopped part-way through sending. Check the cable or network ",
            "and have the machine send the results again, then use Process again here.",
        ),
        ParkedKind::DeliveryRejected => {
            "Fix the problem the lab app reported (see Reason), then use Send again."
        }
        ParkedKind::DeliveryConflict => concat!(
            "The lab app already holds different content for this delivery; Analyte does ",
            "not send it again on its own. Check the delivery in the lab app and settle it ",
            "there, then Mark resolved.",
        ),
        ParkedKind::OrderVersionRefused => concat!(
            "The lab app sent an older or changed copy of an order Analyte already has; ",
            "Analyte kept its copy. Correct the order in the lab app, then Mark resolved.",
        ),
        ParkedKind::CancelConflict => concat!(
            "The lab app cancelled tests the machine has already reported on; Analyte sends ",
            "the machine nothing more. Check both on the machine and in the lab app, then ",
            "Mark resolved.",
        ),
        ParkedKind::PollIntent => concat!(
            "Analyte starts no other exchange with this machine until this item is ",
            "resolved. Check on the machine whether this one finished, then Mark resolved.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::{Identifier, Reason, TextError};

    fn parked(kind: &str, reason: &str) -> Result<Parked, TextError> {
        Ok(Parked {
            kind: Identifier::new(kind)?,
            reason: Reason::new(reason)?,
            targets: Vec::new(),
        })
    }

    // Owner review 2026-11-19: parked categories read in plain words, and the
    // stored category stays on hover; an unknown one shows exactly as written.
    #[test]
    fn categories_read_plainly_and_keep_the_stored_value() -> Result<(), TextError> {
        let mapped = Category::of(&parked("mapping_finding", "synthetic reason")?);
        assert_eq!(mapped.shown, "Test or unit not mapped");
        assert_eq!(mapped.stored, "mapping_finding");
        let failed = Category::of(&parked("interpretation_failure", "synthetic reason")?);
        assert_eq!(failed.shown, "Could not read message");
        let unknown = Category::of(&parked("future_kind", "synthetic reason")?);
        assert_eq!(unknown.shown, "future_kind");
        assert!(unknown.stored.is_empty());
        assert_eq!(unknown.action, Action::ProcessAgain);
        Ok(())
    }

    // Owner decision 2026-11-19 (admin restyle b1): each category keeps its own
    // action, and the conflicting delivery offers Mark resolved, never Send again.
    #[test]
    fn each_category_keeps_its_action_and_a_conflict_is_never_sent_again() {
        for kind in ParkedKind::ALL {
            let expected = match kind {
                ParkedKind::DeliveryRejected => Action::SendAgain,
                ParkedKind::DeliveryConflict
                | ParkedKind::OrderVersionRefused
                | ParkedKind::CancelConflict
                | ParkedKind::PollIntent => Action::Resolve,
                ParkedKind::MappingFinding
                | ParkedKind::InterpretationFailure
                | ParkedKind::ResultReconciliation
                | ParkedKind::IncompleteTransfer => Action::ProcessAgain,
            };
            assert_eq!(main_action(kind), expected, "{kind:?}");
        }
        assert_ne!(main_action(ParkedKind::DeliveryConflict), Action::SendAgain);
    }

    // Owner review 2026-11-19 (Needs review item 3): a mapping finding's advice
    // follows the problem read back from its stored reason, never its wording.
    #[test]
    fn mapping_advice_follows_the_problem_read_back_from_the_reason() {
        let advice = |kind: ParkedKind, reason: &str| {
            let finding = Finding::from_stored(reason);
            guidance(kind, finding.as_ref().map(|f| &f.problem))
        };
        let general = advice(ParkedKind::MappingFinding, "synthetic reason");
        for (reason, starts) in [
            (
                "Position { record: 3, field: 3 }: UnknownTest",
                "The machine sent a test code",
            ),
            (
                "Position { record: 4, field: 5 }: ValueKindMismatch { expected: Coded }",
                "The value does not fit",
            ),
            (
                "Position { record: 2, field: 0 }: MissingSample",
                "The message has no sample ID.",
            ),
            (
                "Position { record: 2, field: 4 }: MissingIdentity { setting: \"LAW order\" }",
                general,
            ),
            (
                "departure-conflict at Position { record: 3, field: 25 }: present non-final status contradicts documented final-only scope",
                general,
            ),
        ] {
            assert!(
                advice(ParkedKind::MappingFinding, reason).starts_with(starts),
                "{reason}"
            );
        }
        assert!(general.starts_with("Check this machine's test mapping"));
        assert!(
            advice(
                ParkedKind::InterpretationFailure,
                "Position { record: 3, field: 3 }: UnknownTest"
            )
            .starts_with("Check the machine's settings")
        );
    }
}
