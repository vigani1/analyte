//! Says who did what in a history line: the recorded actor, the settled action and what a new reading decided.
use super::{Line, amount, moment, moment_unix};
use crate::admin::server::{sessions::ADMIN_KEY_PREFIX, windows::ACTOR_PREFIX};
use crate::operator::recovery;
use analyte_store::RevisionOutcome;
use jiff::Timestamp;

/// Who an audit entry names, in words: the admin pages' admin-key sessions
/// read "admin session" (the session's ID is for support only), a Windows
/// session reads as its account, `DOMAIN\name`, or its SID when Windows gave
/// no name, Analyte's own work reads "Analyte", and any other actor, such as
/// one given to the command line, reads as it was recorded. No person's name
/// is ever made up.
pub(in crate::admin) fn actor(recorded: &str) -> String {
    if recorded.starts_with(ADMIN_KEY_PREFIX) {
        "admin session".into()
    } else if let Some(account) = recorded.strip_prefix(ACTOR_PREFIX) {
        // SIDs hold no ':', so the first one ends the SID.
        account
            .split_once(':')
            .map_or(account, |(_, name)| name)
            .to_owned()
    } else if recorded == "analyte" {
        "Analyte".into()
    } else {
        recorded.to_owned()
    }
}

/// Who recorded an audit entry and why, as one line under it in a history:
/// "admin session · “settled with the lab app”".
pub(in crate::admin) fn recorded(actor: &str, reason: &str) -> String {
    format!("{} · “{reason}”", self::actor(actor))
}

/// What an audited action did to a Needs review item, from the action and
/// targets Analyte recorded with it: crate::admin::view::format::MARKED_RESOLVED, "Processed again:
/// reading 2 recorded" (and, when the new reading still needed review, "it
/// still needs review"), or "Resolved" for an action recorded elsewhere.
pub(in crate::admin) fn settled(action: &str, targets: &[&str]) -> String {
    match action {
        recovery::REPROCESS_ACTION => {
            let reading = targets
                .iter()
                .find_map(|target| target.strip_prefix("interpretation:"))
                .and_then(|rest| rest.rsplit(':').next())
                .map_or_else(String::new, |version| {
                    format!(": reading {version} recorded")
                });
            let still = if targets.contains(&"reprocess:parked") {
                "; it still needs review"
            } else {
                ""
            };
            format!("{}{reading}{still}", settled_action(action))
        }
        _ => settled_action(action).into(),
    }
}

/// [`settled`] in short, without what the new reading decided: "Marked
/// resolved", crate::admin::view::format::PROCESSED_AGAIN, or "Resolved" for an action recorded elsewhere.
pub(in crate::admin) fn settled_action(action: &str) -> &'static str {
    match action {
        recovery::RESOLVE_ACTION => crate::admin::view::format::MARKED_RESOLVED,
        recovery::REPROCESS_ACTION => crate::admin::view::format::PROCESSED_AGAIN,
        analyte_store::RESEND_MEASUREMENT_ACTION => "Recorded as a resend",
        analyte_store::RERUN_MEASUREMENT_ACTION => "Rerun queued",
        analyte_store::SUPERSEDE_SAMPLE_ACTION => "Superseded by the receipt with a tube ID",
        _ => "Resolved",
    }
}

/// One resolved item's line on the Needs review list: "Marked resolved by
/// admin session at 09:20: “Re-sent after the lab app restarted.”"
pub(in crate::admin) fn settled_line(
    what: &str,
    actor: &str,
    seconds: i64,
    reason: &str,
    now: Timestamp,
) -> Line {
    Line::text(format!("{what} by {actor} at "))
        .time(moment_unix(seconds, now))
        .text_part(&format!(": “{reason}”"))
}

/// Where a parked item's message came from and when it arrived:
/// "From cobas-e411 · arrived 10:45".
pub(in crate::admin) fn arrived(machine: &str, at: Timestamp, now: Timestamp) -> Line {
    Line::text(format!("From {machine} · arrived ")).time(moment(at, now))
}

/// What a new reading decided for each of its results, counted: "2 new
/// results, 1 correction". Results Analyte kept for review say so.
pub(in crate::admin) fn outcomes(outcomes: &[RevisionOutcome]) -> String {
    let count = |wanted: fn(&RevisionOutcome) -> bool| {
        u64::try_from(outcomes.iter().filter(|outcome| wanted(outcome)).count()).unwrap_or(u64::MAX)
    };
    let parts: Vec<String> = [
        (
            count(|o| matches!(o, RevisionOutcome::New(_))),
            "new result",
            "new results",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Correction(_))),
            "correction",
            "corrections",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Retransmission(_))),
            "result already received",
            "results already received",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Conflict)),
            "conflicting result kept for review",
            "conflicting results kept for review",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Unresolved)),
            "result without an identity kept for review",
            "results without an identity kept for review",
        ),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| amount(n, one, many))
    .collect();
    if parts.is_empty() {
        "no results in it".into()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decision 2026-11-19 (admin restyle b1): history lines name the
    // admin pages' sessions "admin session" and never a made-up person; other
    // recorded actors read as recorded.
    #[test]
    fn actors_read_as_admin_session_analyte_or_as_recorded() {
        assert_eq!(
            actor("admin-session-0193a1b2-0000-7000-8000-000000000001"),
            "admin session"
        );
        assert_eq!(actor("analyte"), "Analyte");
        assert_eq!(actor("night-shift"), "night-shift");
        assert_eq!(
            recorded("admin-session-1", "settled"),
            "admin session · \u{201c}settled\u{201d}"
        );
    }

    // ADR 0021 §5: a Windows session's actor reads as its account, or as its
    // SID when Windows gave no name; the SID stays only in the record.
    #[test]
    fn windows_actors_read_as_the_account_or_the_sid() {
        assert_eq!(
            actor("windows:S-1-5-21-1-2-3-1001:LAB-PC\\maria"),
            "LAB-PC\\maria"
        );
        assert_eq!(actor("windows:S-1-5-21-1-2-3-1001"), "S-1-5-21-1-2-3-1001");
        assert_eq!(
            recorded("windows:S-1-5-21-1-2-3-1001:LAB-PC\\maria", "settled"),
            "LAB-PC\\maria · \u{201c}settled\u{201d}"
        );
    }

    // Owner decision 2026-11-19 (admin restyle b1): each history line says what
    // the recorded action did, from the targets Analyte stored with it.
    #[test]
    fn settled_actions_read_from_the_stored_action_and_targets() {
        assert_eq!(settled("parked-resolve", &["7"]), "Marked resolved");
        assert_eq!(
            settled(
                "receipt-reprocess",
                &["r", "interpretation:r:3", "reprocess:succeeded", "parked:7"]
            ),
            "Processed again: reading 3 recorded"
        );
        assert_eq!(
            settled(
                "receipt-reprocess",
                &["r", "interpretation:r:2", "reprocess:parked", "parked:7"]
            ),
            "Processed again: reading 2 recorded; it still needs review"
        );
        assert_eq!(settled("receipt-reprocess", &[]), "Processed again");
        assert_eq!(settled("autolyser_poll_custody_complete", &[]), "Resolved");
    }

    // Owner decision 2026-11-19 (admin restyle b1): "Reading N recorded" says
    // what the new reading decided, counted, and what was kept for review.
    #[test]
    fn reprocess_outcomes_are_counted_in_words() {
        use analyte_model::Revision;
        assert_eq!(outcomes(&[]), "no results in it");
        assert_eq!(
            outcomes(&[
                RevisionOutcome::New(Revision(0)),
                RevisionOutcome::New(Revision(0)),
                RevisionOutcome::Correction(Revision(1)),
                RevisionOutcome::Retransmission(Revision(0)),
                RevisionOutcome::Conflict,
                RevisionOutcome::Unresolved,
                RevisionOutcome::Unresolved,
            ]),
            concat!(
                "2 new results, 1 correction, 1 result already received, ",
                "1 conflicting result kept for review, ",
                "2 results without an identity kept for review"
            )
        );
    }

    // Owner review 2026-11-20 (PR 275): a settled item's action in short.
    #[test]
    fn settled_actions_read_in_short() {
        assert_eq!(settled_action("parked-resolve"), "Marked resolved");
        assert_eq!(settled_action("receipt-reprocess"), "Processed again");
        assert_eq!(
            settled_action("autolyser_poll_custody_complete"),
            "Resolved"
        );
    }
}
