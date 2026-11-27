//! Explains why a message needs review in plain words, keeping the stored reason for support.
use crate::machine::{astm, autolyser, hl7, interpretation, law, poct, vidas};
use analyte_config::ValueKind;
use analyte_contract::ReplyProblem;
use analyte_mapping::{Finding, Position, Problem, ReasonPiece, Unidentified};
use analyte_store::{
    CANCEL_CONFLICT_REASON, MODEL_UNKNOWN_REASON, NO_RESULTS_REASON, OrderRefusal, ParkedKind,
    RevisionOutcome, reconciled_outcome,
};

/// A stored reason for people: each mapping position reads "record 3, field 3",
/// or "record 3" when field zero means the whole record; none when the stored
/// reason holds no mapping position, so it already reads as stored. Pages keep
/// the exact stored text beside it, for support.
pub(in crate::admin) fn reason_words(stored: &str) -> Option<String> {
    let pieces = Position::split_stored(stored);
    if !pieces
        .iter()
        .any(|piece| matches!(piece, ReasonPiece::Position(_)))
    {
        return None;
    }
    let mut shown = String::new();
    for piece in pieces {
        match piece {
            ReasonPiece::Text(text) => shown.push_str(text),
            ReasonPiece::Position(position) => shown.push_str(&place(position)),
        }
    }
    Some(shown)
}

/// A Needs review item's stored reason as people read it. Only a reason whose
/// shape Analyte wrote itself gets words: a typed spelling read back exactly
/// (such as a mapping finding) or a fixed sentence the code writes. Any other
/// text, such as a parser diagnostic or the lab app's own detail, has no
/// sentence and shows as stored. Test codes and values are never named; they
/// stay in the protected original message.
pub(in crate::admin) struct Explained {
    /// What the problem is, as the item's title; none when only its category is known.
    pub(in crate::admin) title: Option<&'static str>,
    /// One plain sentence; none when the reason shows as stored.
    pub(in crate::admin) sentence: Option<String>,
    /// Facts for support beside the stored reason, such as the lab app's HTTP status.
    pub(in crate::admin) support: Vec<String>,
}

impl Explained {
    fn said(title: &'static str, sentence: impl Into<String>) -> Self {
        Self {
            title: Some(title),
            sentence: Some(sentence.into()),
            support: Vec::new(),
        }
    }

    const fn as_stored() -> Self {
        Self {
            title: None,
            sentence: None,
            support: Vec::new(),
        }
    }

    fn with_support(mut self, line: String) -> Self {
        self.support.push(line);
        self
    }
}

/// Explains `stored`, the reason of an item of `kind`.
pub(in crate::admin) fn explain(kind: ParkedKind, stored: &str) -> Explained {
    match kind {
        ParkedKind::MappingFinding => mapping_finding(stored),
        ParkedKind::InterpretationFailure => fixed(stored, &READING_REASONS),
        ParkedKind::ResultReconciliation => reconciliation(stored),
        ParkedKind::IncompleteTransfer => fixed(stored, &TRANSFER_REASONS),
        ParkedKind::DeliveryRejected => rejected_delivery(stored),
        // Only HTTP 409 parks a delivery as a conflict (ADR 0008).
        ParkedKind::DeliveryConflict => {
            fixed(stored, &CONFLICT_REASONS).with_support(lab_app_answer("409"))
        }
        ParkedKind::OrderVersionRefused => refused_order(stored),
        ParkedKind::CancelConflict => fixed(stored, &CANCEL_REASONS),
        ParkedKind::PollIntent => fixed(stored, &POLL_REASONS),
        ParkedKind::OrderEventModelUnknown => fixed(stored, &MODEL_REASONS),
    }
}

/// How the lab app answered, under "Details for support": "Lab app answer: HTTP 409".
fn lab_app_answer(status: &str) -> String {
    format!("Lab app answer: HTTP {status}")
}

/// A fixed reason the code writes, its title and its sentence.
type Fixed = (&'static str, &'static str, &'static str);

fn fixed(stored: &str, known: &[Fixed]) -> Explained {
    known
        .iter()
        .find(|(reason, _, _)| *reason == stored)
        .map_or_else(Explained::as_stored, |(_, title, sentence)| {
            Explained::said(title, *sentence)
        })
}

/// Where a mapping finding points: "record 6, field 3", or "record 6" when
/// field zero means the whole record.
fn place(position: Position) -> String {
    match position {
        Position { record, field: 0 } => format!("record {record}"),
        Position { record, field } => format!("record {record}, field {field}"),
    }
}

/// The title and sentence of a mapping problem found at `at`.
fn problem_words(problem: &Problem, at: Position) -> (&'static str, String) {
    let at = place(at);
    match problem {
        Problem::MissingTest => (
            "Test code missing",
            format!("The machine sent no test code in {at}."),
        ),
        Problem::MissingSample => (
            "Sample ID missing",
            format!("The message has no sample ID in {at}."),
        ),
        Problem::MissingIdentity { .. } => (
            IDENTITY_MISSING,
            format!(
                "The value this machine’s settings use to tell results apart is missing in {at}."
            ),
        ),
        Problem::ValueKindMismatch { expected } => (
            "Value is not the expected kind",
            match expected {
                ValueKind::Numeric => format!(
                    "The value in {at} is not a number, but this machine’s definition \
                     expects a number for this test code."
                ),
                ValueKind::Coded => format!(
                    "The value in {at} is not a code, but this machine’s definition \
                     expects a code for this test code."
                ),
                ValueKind::Either => format!(
                    "The value in {at} is neither a number nor a code, but this machine’s \
                     definition expects one of the two for this test code."
                ),
            },
        ),
        Problem::ComponentParts => (
            "Test code parts do not match",
            format!(
                "The test code in {at} does not split into the parts this machine’s \
                 definition names."
            ),
        ),
        Problem::UnsupportedValue => (
            "Value cannot be read",
            format!("The value in {at} is of a kind Analyte cannot read safely."),
        ),
        Problem::UnsupportedFormatting => (
            "Text with formatting codes",
            format!(
                "The text in {at} carries formatting codes; Analyte keeps them as sent and \
                 does not turn them into plain text."
            ),
        ),
        Problem::UnsupportedEscape => (
            "Escape code not agreed",
            format!(
                "The text in {at} uses an escape code that this machine’s settings do not \
                 say how to read."
            ),
        ),
        Problem::UnknownProcessing => (
            "Processing mode unknown",
            format!("The processing mode in {at} is missing or not one Analyte knows."),
        ),
        Problem::ConflictingClassification => (
            "Result classified more than one way",
            format!(
                "This machine’s quality control rules give the result in {at} more than one \
                 classification."
            ),
        ),
        Problem::NoObservations => ("No results in the message", NO_RESULTS.to_owned()),
        Problem::UnsupportedLawStatusOnly => (
            "Status-only update",
            format!(
                "The message in {at} only changes a result’s status, and Analyte cannot \
                 record such updates yet."
            ),
        ),
        Problem::InvalidTime => (
            "Time cannot be read",
            format!("The time in {at} could not be read; the text as sent is kept."),
        ),
        Problem::UnqualifiedHl7Report => (
            "Result report incomplete",
            format!(
                "The result report in {at} lacks a field, or a result status, that a report \
                 must have."
            ),
        ),
        Problem::DepartureConflict => departure_conflict(&at),
        Problem::IncompleteAstmReport => (
            "Incomplete ASTM report",
            "The analyzer did not report normal completion.".to_owned(),
        ),
        Problem::DerivedStatusContract => (DERIVED_STATUS.0, DERIVED_STATUS.1.to_owned()),
    }
}

const NO_RESULTS: &str = "The message holds no results.";

fn departure_conflict(at: &str) -> (&'static str, String) {
    (
        "Result status not final",
        format!(
            "The result status in {at} is not final, but this machine is set up as sending \
             final results only."
        ),
    )
}

const DERIVED_STATUS: (&str, &str) = (
    "Final status cannot be sent",
    concat!(
        "The result’s final status comes from what this machine is documented to send, not ",
        "from the message, and the lab app contract in use cannot say so.",
    ),
);

fn mapping_finding(stored: &str) -> Explained {
    if let Some(finding) = Finding::from_stored(stored) {
        let (title, sentence) = problem_words(&finding.problem, finding.position);
        return Explained::said(title, sentence);
    }
    if let Some(at) = interpretation::departure_conflict_at(stored) {
        let (title, sentence) = departure_conflict(&place(at));
        return Explained::said(title, sentence);
    }
    if stored == interpretation::DERIVED_STATUS_REASON {
        return Explained::said(DERIVED_STATUS.0, DERIVED_STATUS.1);
    }
    Unidentified::from_stored(stored).map_or_else(Explained::as_stored, unidentified)
}

/// Why a result has no identity, in words.
fn unidentified(reason: Unidentified) -> Explained {
    const MISSING: &str = IDENTITY_MISSING;
    match reason {
        Unidentified::Missing { setting, position } => {
            let (title, sentence) = problem_words(&Problem::MissingIdentity { setting }, position);
            Explained::said(title, sentence)
        }
        Unidentified::AutolyserRun => Explained::said(
            MISSING,
            "This machine sends no run number that stays the same, and its patient code is not \
             a trusted sample ID, so Analyte cannot tell this result apart from others.",
        ),
        Unidentified::PoctRepeated => Explained::said(
            "Result identity repeated",
            "Two results in this message would have the same identity, and no sub-ID tells them \
             apart.",
        ),
        Unidentified::PoctEvidence => Explained::said(
            MISSING,
            "The device, time or sequence number that this machine’s settings use to tell \
             results apart is missing.",
        ),
        Unidentified::PoctSequence => Explained::said(
            "Result identity not trusted",
            "This machine’s sequence numbers can start over, so they cannot tell results apart \
             without a trusted run and revision.",
        ),
        Unidentified::VidasLiteral => Explained::said(
            MISSING,
            "This machine’s messages carry no trusted result or run identity, and do not say \
             whether a result is a patient or a quality control result.",
        ),
    }
}

/// The fixed reasons a message that could not be read is parked with.
const READING_REASONS: [Fixed; 15] = [
    (
        NO_RESULTS_REASON,
        "Processing again produced no results",
        "Processing the message again with the current settings produced no results.",
    ),
    (
        astm::CONFLICT_REASON,
        "Piece resent with other content",
        "The machine sent the same numbered piece of a message twice, with different content.",
    ),
    (
        astm::UNSOLICITED_REASON,
        "Piece arrived out of turn",
        "A piece of a message arrived when no transfer from the machine was open.",
    ),
    (
        hl7::UNSOLICITED_REASON,
        "Message not expected",
        "A message arrived that Analyte did not expect, or while too many messages were waiting.",
    ),
    (
        law::INTERRUPTED_REASON,
        "Processing interrupted",
        "Processing of this message stopped before it was confirmed as complete.",
    ),
    (
        law::policy::WORK_CONFLICT_REASON,
        "Returned work differs from the order sent",
        "The work the machine returned differs from the order Analyte sent it.",
    ),
    (
        law::policy::MAPPING_REFUSAL,
        "Results held back for review",
        concat!(
            "Part of this message needs review, and this machine’s results are recorded all ",
            "together or not at all, so none of them were recorded.",
        ),
    ),
    (
        law::policy::BATCH_CONFLICT_REASON,
        "Results conflict with earlier ones",
        concat!(
            "Results in this message conflict with results this machine sent earlier, so none ",
            "of them were recorded.",
        ),
    ),
    (
        poct::LIMIT_REASON,
        "Message too large",
        "The message was larger than the size limit for this machine.",
    ),
    (
        poct::CANCELLED_REASON,
        MESSAGE_CUT_OFF,
        "The connection was stopped while the message was arriving; the part that arrived is kept.",
    ),
    (
        poct::DEADLINE_REASON,
        "Message not finished in time",
        "The machine did not finish sending the message in time; the part that arrived is kept.",
    ),
    (
        poct::INCOMPLETE_REASON,
        MESSAGE_CUT_OFF,
        concat!(
            "The connection timed out or closed before the message was complete; the part that ",
            "arrived is kept.",
        ),
    ),
    (
        poct::DEFERRED_REASON,
        "Message arrived while busy",
        concat!(
            "The message arrived while too many messages were waiting to be stored, so Analyte ",
            "kept it here without reading it.",
        ),
    ),
    (
        vidas::SUFFIX_REASON,
        "Check characters do not match",
        "The check characters at the end of the message do not match its content.",
    ),
    (
        vidas::INCOMPLETE_REASON,
        "Transfer cut off or refused",
        "The transfer from the machine did not finish, or was refused; what arrived is kept.",
    ),
];

/// The fixed reason an edited result is parked with when nothing shows what changed.
const EDITED_REASONS: [Fixed; 1] = [(
    poct::EDIT_GUARD_REASON,
    "Edited result kept for review",
    concat!(
        "The machine sent an edited result whose content looks unchanged, so Analyte cannot ",
        "tell what was edited.",
    ),
)];

/// The fixed reasons an unfinished transfer is parked with.
const TRANSFER_REASONS: [Fixed; 2] = [
    (
        astm::CUT_OFF_REASON,
        "Transfer cut off",
        "The machine stopped sending before the message was complete; the pieces that arrived are kept.",
    ),
    (
        astm::STOPPED_REASON,
        "Transfer cut off by a stop",
        "Analyte stopped while the machine was sending; the pieces it had confirmed are kept.",
    ),
];

/// The fixed reason a conflicting delivery is parked with when the lab app gave no detail.
const CONFLICT_REASONS: [Fixed; 1] = [(
    analyte_sink::CONFLICT_REASON,
    "Lab app already has different content",
    "The lab app already holds different content under this delivery’s ID.",
)];

const CANCEL_REASONS: [Fixed; 1] = [(
    CANCEL_CONFLICT_REASON,
    "Machine’s cancel answers disagree",
    concat!(
        "The machine answered that it could not cancel a test that it had earlier reported ",
        "as cancelled on its own.",
    ),
)];

const MODEL_REASONS: [Fixed; 1] = [(
    MODEL_UNKNOWN_REASON,
    "Order event not sent",
    concat!(
        "This order was stored before Analyte kept each order's machine model, so its ",
        "event to the lab app cannot name the model. The order's state on this page is ",
        "kept; tell the lab app that state.",
    ),
)];

/// What an open request for a machine's next results is called, as its category and its title.
pub(in crate::admin) const POLL_TITLE: &str = "Request for results not settled";

const POLL_REASONS: [Fixed; 1] = [(
    autolyser::POLL_REASON,
    POLL_TITLE,
    concat!(
        "Analyte asked this machine for its next results, and the machine may count them as ",
        "sent before their answer was safely stored.",
    ),
)];

fn reconciliation(stored: &str) -> Explained {
    if stored == poct::EDIT_GUARD_REASON {
        return fixed(stored, &EDITED_REASONS);
    }
    let kept = |what: &str| {
        Explained::said(
            "Result kept for review",
            format!(
                "The result was recorded as {what}, but kept for review: it has no sample ID, \
                 or it holds a value Analyte cannot read safely."
            ),
        )
    };
    match reconciled_outcome(stored) {
        Some(RevisionOutcome::Conflict) => Explained::said(
            "Result differs from an earlier one",
            "A result came with other content under the same identity as an earlier result, \
             and nothing marks it as a correction.",
        ),
        Some(RevisionOutcome::Unresolved) => Explained::said(
            IDENTITY_MISSING,
            "The message does not give this result an identity, so Analyte cannot tell whether \
             it is new, repeated or a correction.",
        ),
        Some(RevisionOutcome::New(_)) => kept("new"),
        Some(RevisionOutcome::Correction(_)) => kept("a correction"),
        Some(RevisionOutcome::Retransmission(_)) => kept("already received"),
        None => Explained::as_stored(),
    }
}

fn rejected_delivery(stored: &str) -> Explained {
    const UNREADABLE: &str = "Lab app’s answer could not be read";
    if stored == analyte_sink::UNSUPPORTED_CONTRACT_REASON {
        return Explained::said(
            "Delivery stored for another contract",
            "The delivery was stored for a lab app contract version that this version of \
             Analyte cannot send.",
        );
    }
    match ReplyProblem::from_stored(stored) {
        Some(ReplyProblem::InvalidSuccess) => Explained::said(
            UNREADABLE,
            "The lab app answered that it took the delivery, but its answer does not follow \
             the lab app contract, so Analyte does not count it as delivered.",
        )
        .with_support(lab_app_answer("200")),
        Some(ReplyProblem::InvalidError) => Explained::said(
            UNREADABLE,
            "The lab app refused the delivery, but its answer does not follow the lab app \
             contract, so its reason is not known.",
        )
        .with_support(lab_app_answer("400 or 422")),
        Some(ReplyProblem::UnexpectedStatus(status)) => Explained::said(
            "Unexpected answer from the lab app",
            "The lab app gave an answer that the lab app contract does not define for a \
             delivery.",
        )
        .with_support(lab_app_answer(&status.to_string())),
        None => Explained::as_stored(),
    }
}

fn refused_order(stored: &str) -> Explained {
    match OrderRefusal::from_stored(stored) {
        Some(OrderRefusal::Older { offered, kept }) => Explained::said(
            "Older order version refused",
            format!(
                "The lab app sent version {offered} of an order after Analyte already had \
                 version {kept}; Analyte kept version {kept}."
            ),
        ),
        Some(OrderRefusal::ChangedContent { kept }) => Explained::said(
            "Changed order refused",
            format!(
                "The lab app sent other content under version {kept} of an order that Analyte \
                 already had; Analyte kept its copy."
            ),
        ),
        None => Explained::as_stored(),
    }
}

/// Heads the details of how a stored message was read.
pub(in crate::admin) const HOW_READ: &str = "How this message was read";

/// Labels the screen for comparing related measurements.
pub(in crate::admin) const COMPARE_MEASUREMENTS: &str = "Compare measurements";

/// Labels the action to mark a review item resolved.
pub(in crate::admin) const MARK_RESOLVED: &str = "Mark resolved";

/// Labels the action to reprocess a review item.
pub(in crate::admin) const PROCESS_AGAIN: &str = "Process again";

/// The recorded action of marking a review item resolved.
pub(in crate::admin) const MARKED_RESOLVED: &str = "Marked resolved";

/// The recorded action of reprocessing a review item.
pub(in crate::admin) const PROCESSED_AGAIN: &str = "Processed again";

/// Labels the link to a stored item.
pub(in crate::admin) const OPEN_ITEM: &str = "Open the item";

/// Labels a linked record in a review item.
pub(in crate::admin) const LINKED_RECORD: &str = "Linked record";

/// Labels the action to show the original pieces.
pub(in crate::admin) const AS_ARRIVED: &str = "Show the pieces as they arrived";

/// Labels the original bytes of a stored message.
pub(in crate::admin) const ORIGINAL_MESSAGE: &str = "Original message";

/// Labels the action to show a stored message.
pub(in crate::admin) const SHOW_ORIGINAL: &str = "Show the original message";

/// Heads reports of missing result identity.
const IDENTITY_MISSING: &str = "Result identity missing";

/// Heads reports of incomplete messages.
const MESSAGE_CUT_OFF: &str = "Message cut off";

#[cfg(test)]
mod tests {
    use super::*;

    // Owner review 2026-11-19 (structure review PR 2): positions read as record
    // and field, and other reasons are unchanged; the stored reason stays beside
    // the words on each page, now under "Details for support" (admin restyle b2).
    #[test]
    fn reasons_show_positions_plainly_and_keep_the_stored_text() {
        assert_eq!(
            reason_words("Position { record: 3, field: 3 }: MissingTest").as_deref(),
            Some("record 3, field 3: MissingTest")
        );
        assert_eq!(
            reason_words(
                "bad <a> at Position { record: 7, field: 0 }, see Position { record: 8, field: 2 }"
            )
            .as_deref(),
            Some("bad <a> at record 7, see record 8, field 2")
        );
        for stored in ["checksum mismatch in frame 2", "Position { record: x }", ""] {
            assert_eq!(reason_words(stored), None, "{stored}");
        }
    }

    // ADR 0018: the restyled pages show a stored reason in plain words with the
    // exact text on hover, by the same rule as the old pages.
    #[test]
    fn reason_words_follow_the_reason_rule() {
        assert_eq!(
            reason_words("Position { record: 3, field: 0 }: MissingSample").as_deref(),
            Some("record 3: MissingSample")
        );
        assert_eq!(reason_words("checksum mismatch"), None);
    }

    /// What `explain` gives for `stored` under `kind`: title, sentence and support.
    fn explained(
        kind: ParkedKind,
        stored: &str,
    ) -> (Option<&'static str>, Option<String>, Vec<String>) {
        let explained = explain(kind, stored);
        (explained.title, explained.sentence, explained.support)
    }

    /// Checks that `stored` reads as `title` and exactly `sentence`, with no support line.
    fn says(kind: ParkedKind, stored: &str, title: &str, sentence: &str) {
        assert_eq!(
            explained(kind, stored),
            (Some(title), Some(sentence.to_owned()), Vec::new()),
            "{stored}"
        );
    }

    /// Each mapping problem, its title and its sentence for record 6, field 3.
    const PROBLEM_WORDS: [(Problem, &str, &str); 18] = [
        (
            Problem::MissingTest,
            "Test code missing",
            "The machine sent no test code in record 6, field 3.",
        ),
        (
            Problem::MissingSample,
            "Sample ID missing",
            "The message has no sample ID in record 6, field 3.",
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Numeric,
            },
            "Value is not the expected kind",
            concat!(
                "The value in record 6, field 3 is not a number, but this machine’s ",
                "definition expects a number for this test code.",
            ),
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Coded,
            },
            "Value is not the expected kind",
            concat!(
                "The value in record 6, field 3 is not a code, but this machine’s ",
                "definition expects a code for this test code.",
            ),
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Either,
            },
            "Value is not the expected kind",
            concat!(
                "The value in record 6, field 3 is neither a number nor a code, but this ",
                "machine’s definition expects one of the two for this test code.",
            ),
        ),
        (
            Problem::ComponentParts,
            "Test code parts do not match",
            concat!(
                "The test code in record 6, field 3 does not split into the parts this ",
                "machine’s definition names.",
            ),
        ),
        (
            Problem::UnsupportedValue,
            "Value cannot be read",
            "The value in record 6, field 3 is of a kind Analyte cannot read safely.",
        ),
        (
            Problem::UnsupportedFormatting,
            "Text with formatting codes",
            concat!(
                "The text in record 6, field 3 carries formatting codes; Analyte keeps them ",
                "as sent and does not turn them into plain text.",
            ),
        ),
        (
            Problem::UnsupportedEscape,
            "Escape code not agreed",
            concat!(
                "The text in record 6, field 3 uses an escape code that this machine’s ",
                "settings do not say how to read.",
            ),
        ),
        (
            Problem::UnknownProcessing,
            "Processing mode unknown",
            "The processing mode in record 6, field 3 is missing or not one Analyte knows.",
        ),
        (
            Problem::ConflictingClassification,
            "Result classified more than one way",
            concat!(
                "This machine’s quality control rules give the result in record 6, field 3 ",
                "more than one classification.",
            ),
        ),
        (
            Problem::NoObservations,
            "No results in the message",
            "The message holds no results.",
        ),
        (
            Problem::UnsupportedLawStatusOnly,
            "Status-only update",
            concat!(
                "The message in record 6, field 3 only changes a result’s status, and ",
                "Analyte cannot record such updates yet.",
            ),
        ),
        (
            Problem::InvalidTime,
            "Time cannot be read",
            "The time in record 6, field 3 could not be read; the text as sent is kept.",
        ),
        (
            Problem::UnqualifiedHl7Report,
            "Result report incomplete",
            concat!(
                "The result report in record 6, field 3 lacks a field, or a result status, ",
                "that a report must have.",
            ),
        ),
        (
            Problem::DepartureConflict,
            "Result status not final",
            concat!(
                "The result status in record 6, field 3 is not final, but this machine is ",
                "set up as sending final results only.",
            ),
        ),
        (
            Problem::DerivedStatusContract,
            "Final status cannot be sent",
            concat!(
                "The result’s final status comes from what this machine is documented to ",
                "send, not from the message, and the lab app contract in use cannot say so.",
            ),
        ),
        (
            Problem::MissingIdentity {
                setting: analyte_mapping::IdentitySetting::OrderId,
            },
            "Result identity missing",
            concat!(
                "The value this machine’s settings use to tell results apart is missing in ",
                "record 6, field 3.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): every mapping problem has one plain
    // sentence and its own title; the position reads as record and field (or
    // the record alone for field zero), and no test code or value is named.
    #[test]
    fn every_mapping_problem_reads_as_one_plain_sentence() {
        use analyte_mapping::IdentitySetting;
        let finding = ParkedKind::MappingFinding;
        for (problem, title, sentence) in PROBLEM_WORDS {
            let stored = format!("Position {{ record: 6, field: 3 }}: {problem}");
            says(finding, &stored, title, sentence);
        }
        // Field zero means the whole record.
        says(
            finding,
            "Position { record: 2, field: 0 }: MissingSample",
            "Sample ID missing",
            "The message has no sample ID in record 2.",
        );
        // Every identity setting reads back, and the setting name is for support only.
        for setting in IdentitySetting::ALL {
            let stored = format!(
                "Position {{ record: 4, field: 2 }}: {}",
                Problem::MissingIdentity { setting }
            );
            let (title, sentence, _) = explained(finding, &stored);
            assert_eq!(title, Some("Result identity missing"), "{stored}");
            assert!(
                !sentence.unwrap_or_default().contains(setting.as_str()),
                "{stored}"
            );
        }
    }

    /// Each reason a result has no identity, its title and its sentence.
    const UNIDENTIFIED_WORDS: [(Unidentified, &str, &str); 6] = [
        (
            Unidentified::Missing {
                setting: analyte_mapping::IdentitySetting::Run,
                position: Position {
                    record: 5,
                    field: 9,
                },
            },
            "Result identity missing",
            concat!(
                "The value this machine’s settings use to tell results apart is missing in ",
                "record 5, field 9.",
            ),
        ),
        (
            Unidentified::AutolyserRun,
            "Result identity missing",
            concat!(
                "This machine sends no run number that stays the same, and its patient code ",
                "is not a trusted sample ID, so Analyte cannot tell this result apart from ",
                "others.",
            ),
        ),
        (
            Unidentified::PoctRepeated,
            "Result identity repeated",
            concat!(
                "Two results in this message would have the same identity, and no sub-ID ",
                "tells them apart.",
            ),
        ),
        (
            Unidentified::PoctEvidence,
            "Result identity missing",
            concat!(
                "The device, time or sequence number that this machine’s settings use to ",
                "tell results apart is missing.",
            ),
        ),
        (
            Unidentified::PoctSequence,
            "Result identity not trusted",
            concat!(
                "This machine’s sequence numbers can start over, so they cannot tell results ",
                "apart without a trusted run and revision.",
            ),
        ),
        (
            Unidentified::VidasLiteral,
            "Result identity missing",
            concat!(
                "This machine’s messages carry no trusted result or run identity, and do not ",
                "say whether a result is a patient or a quality control result.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): the other mapping findings the code
    // writes (a departure conflict, the derived-status sentence and each reason
    // a result has no identity) read plainly; anything else shows as stored.
    #[test]
    fn other_mapping_findings_read_plainly_and_free_text_shows_as_stored() {
        let finding = ParkedKind::MappingFinding;
        says(
            finding,
            concat!(
                "departure-conflict at Position { record: 3, field: 25 }: present non-final ",
                "status contradicts documented final-only scope",
            ),
            "Result status not final",
            concat!(
                "The result status in record 3, field 25 is not final, but this machine is set ",
                "up as sending final results only.",
            ),
        );
        says(
            finding,
            interpretation::DERIVED_STATUS_REASON,
            "Final status cannot be sent",
            concat!(
                "The result’s final status comes from what this machine is documented to send, ",
                "not from the message, and the lab app contract in use cannot say so.",
            ),
        );
        for (reason, title, sentence) in UNIDENTIFIED_WORDS {
            says(finding, &reason.to_string(), title, sentence);
        }
        for stored in [
            "Position { record: 3, field: 3 }: missingtest",
            // ADR 0029 removed the test table: these old spellings show as stored.
            "Position { record: 3, field: 3 }: UnknownTest",
            "unknown test mapping",
            "missing identity something else at record 1 field 1",
            "a parser said something",
        ] {
            assert_eq!(
                explained(finding, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
        // A finding's spelling under another kind is not read as a finding.
        assert_eq!(
            explained(
                ParkedKind::InterpretationFailure,
                "Position { record: 3, field: 3 }: MissingTest"
            ),
            (None, None, Vec::new())
        );
    }

    /// Each fixed reason the code writes, under its kind, with its title and sentence.
    const FIXED_WORDS: [(ParkedKind, &str, &str, &str); 20] = [
        (
            ParkedKind::InterpretationFailure,
            NO_RESULTS_REASON,
            "Processing again produced no results",
            "Processing the message again with the current settings produced no results.",
        ),
        (
            ParkedKind::InterpretationFailure,
            astm::CONFLICT_REASON,
            "Piece resent with other content",
            "The machine sent the same numbered piece of a message twice, with different content.",
        ),
        (
            ParkedKind::InterpretationFailure,
            astm::UNSOLICITED_REASON,
            "Piece arrived out of turn",
            "A piece of a message arrived when no transfer from the machine was open.",
        ),
        (
            ParkedKind::InterpretationFailure,
            hl7::UNSOLICITED_REASON,
            "Message not expected",
            "A message arrived that Analyte did not expect, or while too many messages were waiting.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::INTERRUPTED_REASON,
            "Processing interrupted",
            "Processing of this message stopped before it was confirmed as complete.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::WORK_CONFLICT_REASON,
            "Returned work differs from the order sent",
            "The work the machine returned differs from the order Analyte sent it.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::MAPPING_REFUSAL,
            "Results held back for review",
            concat!(
                "Part of this message needs review, and this machine’s results are recorded ",
                "all together or not at all, so none of them were recorded.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::BATCH_CONFLICT_REASON,
            "Results conflict with earlier ones",
            concat!(
                "Results in this message conflict with results this machine sent earlier, so ",
                "none of them were recorded.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::LIMIT_REASON,
            "Message too large",
            "The message was larger than the size limit for this machine.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::CANCELLED_REASON,
            "Message cut off",
            "The connection was stopped while the message was arriving; the part that arrived is kept.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::DEADLINE_REASON,
            "Message not finished in time",
            "The machine did not finish sending the message in time; the part that arrived is kept.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::INCOMPLETE_REASON,
            "Message cut off",
            concat!(
                "The connection timed out or closed before the message was complete; the ",
                "part that arrived is kept.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::DEFERRED_REASON,
            "Message arrived while busy",
            concat!(
                "The message arrived while too many messages were waiting to be stored, so ",
                "Analyte kept it here without reading it.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            vidas::SUFFIX_REASON,
            "Check characters do not match",
            "The check characters at the end of the message do not match its content.",
        ),
        (
            ParkedKind::InterpretationFailure,
            vidas::INCOMPLETE_REASON,
            "Transfer cut off or refused",
            "The transfer from the machine did not finish, or was refused; what arrived is kept.",
        ),
        (
            ParkedKind::IncompleteTransfer,
            astm::CUT_OFF_REASON,
            "Transfer cut off",
            concat!(
                "The machine stopped sending before the message was complete; the pieces ",
                "that arrived are kept.",
            ),
        ),
        (
            ParkedKind::IncompleteTransfer,
            astm::STOPPED_REASON,
            "Transfer cut off by a stop",
            "Analyte stopped while the machine was sending; the pieces it had confirmed are kept.",
        ),
        (
            ParkedKind::CancelConflict,
            CANCEL_CONFLICT_REASON,
            "Machine’s cancel answers disagree",
            concat!(
                "The machine answered that it could not cancel a test that it had earlier ",
                "reported as cancelled on its own.",
            ),
        ),
        (
            ParkedKind::OrderEventModelUnknown,
            MODEL_UNKNOWN_REASON,
            "Order event not sent",
            concat!(
                "This order was stored before Analyte kept each order's machine model, so its ",
                "event to the lab app cannot name the model. The order's state on this page is ",
                "kept; tell the lab app that state.",
            ),
        ),
        (
            ParkedKind::PollIntent,
            autolyser::POLL_REASON,
            "Request for results not settled",
            concat!(
                "Analyte asked this machine for its next results, and the machine may count ",
                "them as sent before their answer was safely stored.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): each fixed sentence the code writes for
    // a message that could not be read, an unfinished transfer, a cancel
    // conflict or an unsettled request reads plainly; other text shows as stored.
    #[test]
    fn fixed_reasons_the_code_writes_read_plainly() {
        let reading = ParkedKind::InterpretationFailure;
        for (kind, stored, title, sentence) in FIXED_WORDS {
            says(kind, stored, title, sentence);
        }
        for kind in [
            reading,
            ParkedKind::IncompleteTransfer,
            ParkedKind::CancelConflict,
            ParkedKind::PollIntent,
            ParkedKind::OrderEventModelUnknown,
        ] {
            assert_eq!(
                explained(kind, "POCT syntax: unexpected end"),
                (None, None, Vec::new())
            );
        }
        // A fixed sentence under another kind is not read as that kind's.
        assert_eq!(
            explained(ParkedKind::PollIntent, astm::CUT_OFF_REASON),
            (None, None, Vec::new())
        );
    }

    // Owner review 2026-11-20 (PR 275): a parked result's reconciliation
    // outcome and the edit guard read plainly; the revision number stays in the
    // stored reason under support.
    #[test]
    fn result_reconciliation_reasons_read_plainly() {
        let result = ParkedKind::ResultReconciliation;
        let kept = |what: &str| {
            format!(
                "The result was recorded as {what}, but kept for review: it has no sample ID, or \
                 it holds a value Analyte cannot read safely."
            )
        };
        for (stored, title, sentence) in [
            (
                "result reconciliation: Conflict".to_owned(),
                "Result differs from an earlier one",
                concat!(
                    "A result came with other content under the same identity as an earlier ",
                    "result, and nothing marks it as a correction.",
                )
                .to_owned(),
            ),
            (
                "result reconciliation: Unresolved".to_owned(),
                "Result identity missing",
                concat!(
                    "The message does not give this result an identity, so Analyte cannot tell ",
                    "whether it is new, repeated or a correction.",
                )
                .to_owned(),
            ),
            (
                "result reconciliation: New(Revision(0))".to_owned(),
                "Result kept for review",
                kept("new"),
            ),
            (
                "result reconciliation: Correction(Revision(2))".to_owned(),
                "Result kept for review",
                kept("a correction"),
            ),
            (
                "result reconciliation: Retransmission(Revision(1))".to_owned(),
                "Result kept for review",
                kept("already received"),
            ),
            (
                poct::EDIT_GUARD_REASON.to_owned(),
                "Edited result kept for review",
                concat!(
                    "The machine sent an edited result whose content looks unchanged, so Analyte ",
                    "cannot tell what was edited.",
                )
                .to_owned(),
            ),
        ] {
            says(result, &stored, title, &sentence);
        }
        for stored in [
            "result reconciliation: New(Revision(01))",
            "result reconciliation: conflict",
            "multiple final runs under the same order and test",
        ] {
            assert_eq!(
                explained(result, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
    }

    // Owner review 2026-11-20 (PR 275): a rejected delivery's reason reads
    // plainly when Analyte wrote it, with the HTTP status under support and
    // never in the sentence; the lab app's own detail shows as stored.
    #[test]
    fn delivery_and_order_reasons_keep_http_codes_for_support() {
        let rejected = ParkedKind::DeliveryRejected;
        let unreadable = "Lab app’s answer could not be read";
        for (stored, title, sentence, support) in [
            (
                "invalid success response",
                unreadable,
                concat!(
                    "The lab app answered that it took the delivery, but its answer does not ",
                    "follow the lab app contract, so Analyte does not count it as delivered.",
                ),
                "Lab app answer: HTTP 200",
            ),
            (
                "invalid error response",
                unreadable,
                concat!(
                    "The lab app refused the delivery, but its answer does not follow the lab app ",
                    "contract, so its reason is not known.",
                ),
                "Lab app answer: HTTP 400 or 422",
            ),
            (
                "unexpected HTTP status 418",
                "Unexpected answer from the lab app",
                "The lab app gave an answer that the lab app contract does not define for a delivery.",
                "Lab app answer: HTTP 418",
            ),
        ] {
            assert_eq!(
                explained(rejected, stored),
                (
                    Some(title),
                    Some(sentence.to_owned()),
                    vec![support.to_owned()]
                ),
                "{stored}"
            );
            assert!(!sentence.contains("HTTP"));
        }
        says(
            rejected,
            analyte_sink::UNSUPPORTED_CONTRACT_REASON,
            "Delivery stored for another contract",
            concat!(
                "The delivery was stored for a lab app contract version that this version of ",
                "Analyte cannot send.",
            ),
        );
        for stored in ["sample 12 is unknown", "unexpected HTTP status 0418"] {
            assert_eq!(
                explained(rejected, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
        // Only HTTP 409 parks a conflict: every one says so under support.
        let conflict = ParkedKind::DeliveryConflict;
        let http_409 = vec!["Lab app answer: HTTP 409".to_owned()];
        assert_eq!(
            explained(conflict, analyte_sink::CONFLICT_REASON),
            (
                Some("Lab app already has different content"),
                Some(
                    "The lab app already holds different content under this delivery’s ID."
                        .to_owned()
                ),
                http_409.clone()
            )
        );
        assert_eq!(
            explained(conflict, "result 7 already stored with other values"),
            (None, None, http_409)
        );
        let order = ParkedKind::OrderVersionRefused;
        says(
            order,
            "app sent order version 2 after cached version 5; kept 5",
            "Older order version refused",
            concat!(
                "The lab app sent version 2 of an order after Analyte already had version 5; ",
                "Analyte kept version 5.",
            ),
        );
        says(
            order,
            "app sent different content for order version 3; kept the cached one",
            "Changed order refused",
            concat!(
                "The lab app sent other content under version 3 of an order that Analyte already ",
                "had; Analyte kept its copy.",
            ),
        );
        assert_eq!(
            explained(
                order,
                "app sent order version 2 after cached version 5; kept 6"
            ),
            (None, None, Vec::new())
        );
    }
}
