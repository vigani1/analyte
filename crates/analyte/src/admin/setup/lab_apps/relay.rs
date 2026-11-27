//! Serves `POST /command/lab-apps`: the Lab apps page's add and save, keys included, for `lab-apps add` and `lab-apps keys` when only the running engine may write the locked keys file (ADR 0028); and what a save says in the command line's words.
//!
//! The command sends this only to a listener that proved it holds the admin
//! key (ADR 0025), with that key as a `Bearer` header; the server admits a
//! `/command/` route only so, never with a session cookie. The body is read
//! once, bounded by [`LIMIT`], and refused on any unknown field. The keys move
//! into the page's own checks ([`add_blocking`], [`save_blocking`]) and from
//! there into the locked file; neither the body nor a key is logged or answered.
use super::change::{Change, Entered, add_blocking, reason, save_blocking, saved_lines};
use crate::admin::server::Context;
use crate::admin::setup::{SetupError, Via, audit_as, off_thread};
use analyte_i18n::{message as tr_args, text as tr};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

/// Where the command sends a lab app and its keys.
pub(in crate::admin) const PATH: &str = "/command/lab-apps";
/// The largest body read: an address and two keys fit many times over.
pub(in crate::admin) const LIMIT: usize = 16_384;

/// What the command sends: who, which change, and the Lab apps form's values,
/// keys included. Deliberately has no `Debug`: it holds keys.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Sent {
    /// The command's `--actor`, checked as the command line checks it.
    actor: String,
    change: Change,
    name: String,
    base_url: String,
    freshness_seconds: String,
    request_timeout_seconds: String,
    #[serde(default)]
    review_notices: String,
    api_key: String,
    signing_key: String,
}

impl Sent {
    /// The change `change` of `entered`, made by `actor`.
    pub(in crate::admin::setup) fn new(actor: &str, change: Change, entered: Entered) -> Self {
        Self {
            actor: actor.to_owned(),
            change,
            name: entered.name,
            base_url: entered.base_url,
            freshness_seconds: entered.freshness_seconds,
            request_timeout_seconds: entered.request_timeout_seconds,
            review_notices: entered.review_notices,
            api_key: entered.api_key,
            signing_key: entered.signing_key,
        }
    }

    fn entered(self) -> Entered {
        Entered {
            name: self.name,
            base_url: self.base_url,
            freshness_seconds: self.freshness_seconds,
            request_timeout_seconds: self.request_timeout_seconds,
            review_notices: self.review_notices,
            api_key: self.api_key,
            signing_key: self.signing_key,
        }
    }
}

/// The engine's answer: what was saved, in the lines the command prints, or
/// the page's refusal. Never a key value.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(in crate::admin::setup) enum Answer {
    Saved {
        /// What the save wrote, as [`saved_lines`] says it.
        lines: Vec<String>,
        /// Why the audit row was not written; the change itself stays, as on the page.
        audit_failed: Option<String>,
    },
    Refused {
        /// The page's reason.
        reason: String,
        /// The settings were written but the keys were not.
        keys_not_stored: bool,
    },
}

/// A request that could not be read; the reason names no value it held.
fn unreadable(status: StatusCode, reason: &str) -> Response {
    (
        status,
        Json(Answer::Refused {
            reason: reason.to_owned(),
            keys_not_stored: false,
        }),
    )
        .into_response()
}

/// POST /command/lab-apps: the Lab apps page's add or save, then its audit row.
pub(in crate::admin) async fn post(
    State(state): State<Arc<Context>>,
    sent: Result<Json<Sent>, JsonRejection>,
) -> Response {
    // Only the rejection's kind: a reader's own words could quote a value.
    let sent = match sent {
        Ok(Json(sent)) => sent,
        Err(rejection) => {
            let status = rejection.status();
            let response_status = if status == StatusCode::PAYLOAD_TOO_LARGE {
                status
            } else {
                StatusCode::BAD_REQUEST
            };
            return unreadable(
                response_status,
                &tr_args(
                    "setup-the-engine-could-not-read-the-request",
                    [("status", (status).to_string().into())],
                ),
            );
        }
    };
    let Ok(actor) = crate::admin::command_line_actor(&sent.actor) else {
        return unreadable(
            StatusCode::BAD_REQUEST,
            tr("setup-the-command-named-no-valid-actor"),
        );
    };
    let change = sent.change;
    let name = sent.name.clone();
    let entered = sent.entered();
    let saved = off_thread(&state.setup, move |setup| match change {
        Change::Added => add_blocking(setup, entered),
        Change::Saved => save_blocking(setup, &name, entered),
    })
    .await;
    let saved = match saved {
        Ok(saved) => saved,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(Answer::Refused {
                    reason: error.to_string(),
                    keys_not_stored: matches!(error, SetupError::KeysNotStored(_)),
                }),
            )
                .into_response();
        }
    };
    let lab_app = saved.settings.name.as_str();
    let audit_failed = audit_as(
        &state.store,
        &actor,
        change.action(),
        lab_app,
        &reason(Via::CommandLine, change, &saved),
    )
    .await
    .err()
    .map(|error| error.to_string());
    tracing::info!(
        lab_app,
        "the command line {} a lab app through the engine",
        change.verb()
    );
    Json(Answer::Saved {
        lines: saved_lines(change, &saved),
        audit_failed,
    })
    .into_response()
}
