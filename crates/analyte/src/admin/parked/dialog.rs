//! Serves the Mark resolved and Process again dialogs (GET and POST /parked/{id}/resolve and /reprocess): drawn over the item page, a real form with a required reason, audited when sent.
//!
//! No script: the dialog is the item page with a `.scrim`, its Cancel is a
//! link back to the item, and the form posts with the session's form token.
//! The shared request guard already refused a post without a session or with
//! a wrong token; these handlers check again and ask for a signed-in session.
use super::{ItemQuery, ItemRead, item_page, open_count};
use crate::admin::server::Context;
use crate::admin::server::sessions::{Session, signed_in};
use crate::admin::view::parts::ReasonDialog;
use crate::admin::view::report::{Button, Problem, Report, Section, read_within, respond};
use crate::admin::view::{Tab, Tone, format, frame::Frame};
use crate::admin::{CommandError, Error};
use crate::operator::recovery;
use analyte_model::{Identifier, Reason};
use analyte_store::{InterpretationCommit, OperatorError, RevisionOutcome};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

/// Which audited action a dialog confirms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confirm {
    Resolve,
    Reprocess,
}

impl Confirm {
    /// The dialog over item `id`, with what was typed and why it was refused.
    fn dialog(
        self,
        id: i64,
        session: &Session,
        reason: &str,
        error: Option<&'static str>,
    ) -> ReasonDialog {
        let (path, title, text, placeholder, button) = match self {
            Self::Resolve => (
                "resolve",
                "Mark as resolved?",
                concat!(
                    "The item leaves the open list and stays under Resolved with its history. ",
                    "Nothing is deleted, and nothing is sent to a machine or the lab app.",
                ),
                "For example: settled with the lab app team",
                crate::admin::view::format::MARK_RESOLVED,
            ),
            Self::Reprocess => (
                "reprocess",
                "Process this message again?",
                concat!(
                    "Analyte reads the original message again with the current settings. The ",
                    "earlier reading is kept and a new one is added; corrected results follow ",
                    "the usual correction rules. Nothing is sent to the machine.",
                ),
                "For example: machine definition corrected",
                crate::admin::view::format::PROCESS_AGAIN,
            ),
        };
        ReasonDialog {
            action: format!("/parked/{id}/{path}"),
            title,
            text: text.into(),
            placeholder,
            hint: "Required, up to 256 characters. Recorded in the history with the time.",
            button,
            csrf: session.csrf.form_value().to_owned(),
            cancel: format::parked_href(id),
            reason: reason.to_owned(),
            error,
        }
    }
}

/// The form both dialogs post.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct ReasonForm {
    csrf: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    confirm: String,
}

pub(in crate::admin) use crate::operator::recovery::measurement::checked_reason as checked;

pub(in crate::admin) async fn confirm_resolve(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(session) = signed_in(session.as_ref()) else {
        return Problem::SignIn.page(&state, None, Section::NeedsReview(Some(id)));
    };
    open(&state, session, id, Confirm::Resolve, "", None).await
}

pub(in crate::admin) async fn confirm_reprocess(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(session) = signed_in(session.as_ref()) else {
        return Problem::SignIn.page(&state, None, Section::NeedsReview(Some(id)));
    };
    open(&state, session, id, Confirm::Reprocess, "", None).await
}

/// The item page under `confirm`'s dialog: OK on a first look, 400 with
/// `error` when a sent reason was refused. A resolved item, and for Process
/// again an item without exactly one stored message, are refused instead.
/// Finding that message may rebuild an aborted transfer's receipt from its
/// acknowledged frames, as the command line does.
async fn open(
    state: &Context,
    session: &Session,
    id: i64,
    confirm: Confirm,
    typed: &str,
    error: Option<&'static str>,
) -> Response {
    // Finding the message may rebuild a receipt, a write, so this is a request
    // that runs to its end, not a read that stops with the page.
    let read = read_within(async {
        let query = ItemQuery::new(id, 0)?;
        Ok::<_, Error>(
            state
                .store
                .request::<_, CommandError>(crate::deadline()?, move |s| {
                    let Some(read) = query.run(s)? else {
                        return Ok(None);
                    };
                    let receipt = (read.resolution.is_none() && confirm == Confirm::Reprocess)
                        .then(|| s.parked_receipt(id).map(drop));
                    Ok(Some((read, receipt)))
                })
                .await?,
        )
    })
    .await;
    let (read, receipt): (ItemRead, _) = match read {
        Ok(Some(read)) => read,
        Ok(None) => {
            return Problem::NotFound.page(state, Some(session), Section::NeedsReview(Some(id)));
        }
        Err(problem) => return problem.page(state, Some(session), Section::NeedsReview(Some(id))),
    };
    if read.resolution.is_some() {
        return Problem::AlreadyResolved.page(state, Some(session), Section::NeedsReview(Some(id)));
    }
    match receipt {
        Some(Err(OperatorError::Purged)) => {
            return Problem::Purged.page(state, Some(session), Section::NeedsReview(Some(id)));
        }
        Some(Err(other)) => {
            return Problem::NoSingleMessage(other.to_string()).page(
                state,
                Some(session),
                Section::NeedsReview(Some(id)),
            );
        }
        Some(Ok(())) | None => {}
    }
    let dialog = confirm.dialog(id, session, typed, error);
    let html = item_page(state, Some(session), &read, Some(dialog), Timestamp::now()).render();
    let status = if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    respond(
        state,
        Some(session),
        Section::NeedsReview(Some(id)),
        status,
        html,
    )
}

pub(in crate::admin) async fn resolve(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<ReasonForm>,
) -> Response {
    send(&state, &session, id, Confirm::Resolve, &form).await
}

pub(in crate::admin) async fn reprocess(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<ReasonForm>,
) -> Response {
    send(&state, &session, id, Confirm::Reprocess, &form).await
}

/// Runs a sent dialog's action after checking, before anything runs, a
/// signed-in session, its form token and the confirmation, then the reason; a
/// refused reason opens the dialog again with what was typed.
async fn send(
    state: &Context,
    session: &Session,
    id: i64,
    confirm: Confirm,
    form: &ReasonForm,
) -> Response {
    let Some(actor) = session.actor().cloned() else {
        return Problem::SignIn.page(state, None, Section::NeedsReview(Some(id)));
    };
    if !session.csrf_valid(&form.csrf) || form.confirm != "yes" {
        return Problem::FormExpired.page(state, Some(session), Section::NeedsReview(Some(id)));
    }
    let reason = match checked(&form.reason) {
        Ok(reason) => reason,
        Err(error) => return open(state, session, id, confirm, &form.reason, Some(error)).await,
    };
    let done = match confirm {
        Confirm::Resolve => recovery::resolve(&state.store, id, actor, reason)
            .await
            .map(|()| Done::Resolved)
            .map_err(Error::from),
        Confirm::Reprocess => process_again(state, id, actor, reason)
            .await
            .map(Done::Reprocessed),
    };
    match done {
        Ok(done) => done.page(state, session, id).await,
        Err(error) => Problem::Failed(error.to_string()).page(
            state,
            Some(session),
            Section::NeedsReview(Some(id)),
        ),
    }
}

/// Reads item `id`'s original message again with the program's settings as
/// they are on disk now, refusing when they name another store.
async fn process_again(
    state: &Context,
    id: i64,
    actor: Identifier,
    reason: Reason,
) -> Result<InterpretationCommit, Error> {
    let program =
        super::program_for_store(state.setup.config().to_owned(), state.directory.clone()).await?;
    recovery::reprocess(
        &state.store,
        &program,
        recovery::ReprocessTarget::Parked(id),
        actor,
        reason,
    )
    .await
    .map_err(Error::from)
}

/// What a sent dialog did.
enum Done {
    Resolved,
    Reprocessed(InterpretationCommit),
}

impl Done {
    /// The page saying what was recorded, with the way back.
    async fn page(self, state: &Context, session: &Session, id: i64) -> Response {
        let (title, tone, alert, text) = match self {
            Self::Resolved => (
                crate::admin::view::format::MARKED_RESOLVED,
                Tone::Good,
                format!("Item {id} is marked resolved"),
                String::from(concat!(
                    "It moved to Resolved and keeps its history. Nothing was sent to a ",
                    "machine or the lab app.",
                )),
            ),
            Self::Reprocessed(commit) => {
                let review = commit.outcomes.iter().any(|outcome| {
                    matches!(
                        outcome,
                        RevisionOutcome::Conflict | RevisionOutcome::Unresolved
                    )
                });
                (
                    crate::admin::view::format::PROCESSED_AGAIN,
                    if review { Tone::Attention } else { Tone::Good },
                    format!("Reading {} was recorded", commit.version),
                    format!(
                        "What happened: {}. The earlier reading is kept.",
                        format::outcomes(&commit.outcomes)
                    ),
                )
            }
        };
        let section = Section::NeedsReview(Some(id));
        let page = Report {
            links: vec![
                Button {
                    href: Tab::NeedsReview.href().to_owned(),
                    label: crate::admin::view::Tab::NeedsReview.back(),
                    class: "btn--primary",
                },
                Button {
                    href: format::parked_href(id),
                    label: crate::admin::view::format::OPEN_ITEM,
                    class: "btn--outline",
                },
            ],
            ..Report::new(
                Frame::new(
                    state,
                    Tab::NeedsReview,
                    title,
                    open_count(state).await,
                    Some(session),
                    Timestamp::now(),
                ),
                section,
                tone,
                alert,
                text,
            )
        };
        respond(state, Some(session), section, StatusCode::OK, page.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decision 2026-11-19 (admin restyle b1): the reason is required and
    // capped at 256 characters; line breaks typed in the field become spaces,
    // because an audit reason is one line.
    #[test]
    fn reasons_are_required_one_line_and_at_most_256_characters() {
        for blank in ["", "   ", "\r\n\t"] {
            assert_eq!(
                checked(blank).err(),
                Some("Write a reason. It is recorded in the history."),
                "{blank:?}"
            );
        }
        assert_eq!(
            checked("first line\r\nsecond\nthird\tend").map(|r| r.as_str().to_owned()),
            Ok("first line second third end".to_owned())
        );
        let longest = "é".repeat(256);
        assert_eq!(
            checked(&longest).map(|r| r.as_str().to_owned()),
            Ok(longest.clone())
        );
        assert_eq!(
            checked(&format!("{longest}x")).err(),
            Some("Keep the reason to 256 characters.")
        );
        assert_eq!(
            checked("bell \u{7}").err(),
            Some("The reason holds a character that cannot be recorded.")
        );
    }
}
