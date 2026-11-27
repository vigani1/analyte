//! Shows the audited comparison and accepts the signed-in operator's explicit decision.
//!
//! A correction replaces a value in the lab app, so its button first draws a
//! confirmation over the comparison with the earlier and new values side by
//! side; only the confirmation's own post records the decision.
use crate::admin::server::{
    Context,
    sessions::{Session, signed_in},
};
use crate::admin::view::{
    Tab,
    frame::Frame,
    report::{Problem, Section, respond},
};
use crate::operator::recovery::measurement;
use analyte_i18n::text as tr;
use analyte_model::ResultContent;
use analyte_store::MeasurementDecision;
use askama::Template;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Template)]
#[template(path = "pages/parked/measurement.html")]
struct Page {
    frame: Frame,
    id: i64,
    snapshot: String,
    csrf: String,
    sample: Option<String>,
    held: Evidence,
    other: Evidence,
    /// Whether the held reading may be confirmed as the next revision.
    correctable: bool,
    /// The correction confirmation drawn over the page, with the typed reason.
    confirm: Option<String>,
}

struct Evidence {
    receipt: String,
    sample: Option<String>,
    observations: Vec<Observation>,
    exact: String,
}

struct Observation {
    test: String,
    values: Vec<String>,
    times: Vec<String>,
    status: String,
}

impl Evidence {
    fn from_results(results: &[ResultContent]) -> Result<Option<Self>, serde_json::Error> {
        let Some(first) = results.first() else {
            return Ok(None);
        };
        let observations = results
            .iter()
            .flat_map(|result| result.parts.as_slice())
            .map(|part| Observation {
                test: part.test.value.as_str().to_owned(),
                values: part
                    .values
                    .as_slice()
                    .iter()
                    .map(|value| value.original.as_str().to_owned())
                    .collect(),
                times: part
                    .times
                    .as_slice()
                    .iter()
                    .map(|time| format!("{} {}", time.role.as_str(), time.time.original.as_str()))
                    .collect(),
                status: part.source_status.as_str().to_owned(),
            })
            .collect();
        Ok(Some(Self {
            receipt: uuid::Uuid::from(first.receipt).to_string(),
            sample: first
                .sample
                .as_ref()
                .map(|sample| sample.id.as_str().to_owned()),
            observations,
            exact: serde_json::to_string_pretty(results)?,
        }))
    }
}

pub(in crate::admin) async fn show(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let section = Section::NeedsReview(Some(id));
    let Some(session) = signed_in(session.as_ref()) else {
        return Problem::SignIn.page(&state, None, section);
    };
    compared(&state, id, session, None).await
}

/// The audited comparison, with the correction confirmation over it when
/// `confirm` holds the submitted snapshot and reason. A confirmation whose
/// snapshot no longer matches is refused, so nobody confirms changed values.
async fn compared(
    state: &Context,
    id: i64,
    session: &Session,
    confirm: Option<(&str, String)>,
) -> Response {
    let section = Section::NeedsReview(Some(id));
    let Some(actor) = session.actor().cloned() else {
        return Problem::SignIn.page(state, None, section);
    };
    let review = match measurement::review(&state.store, id, actor).await {
        Ok(Some(review)) => review,
        Ok(None) => {
            return Problem::Failed(
                tr("admin-no-eligible-measurement-comparison-is-available").into(),
            )
            .page(state, Some(session), section);
        }
        Err(error) => {
            return Problem::Failed(error.to_string()).page(state, Some(session), section);
        }
    };
    let (Ok(Some(held)), Ok(Some(other))) = (
        Evidence::from_results(&review.held),
        Evidence::from_results(&review.other),
    ) else {
        return Problem::Failed(tr("admin-the-retained-results-could-not-be-displayed").into())
            .page(state, Some(session), section);
    };
    if let Some((snapshot, _)) = &confirm
        && (*snapshot != review.snapshot || !review.correctable)
    {
        return Problem::Failed(analyte_store::ReconciliationError::ReviewChanged.to_string())
            .page(state, Some(session), section);
    }
    let sample = if review.suggestion {
        other.sample.clone()
    } else {
        None
    };
    let page = Page {
        frame: Frame::new(
            state,
            Tab::NeedsReview,
            crate::admin::view::format::compare_measurements(),
            None,
            Some(session),
            Timestamp::now(),
        ),
        id,
        snapshot: review.snapshot,
        csrf: session.csrf.form_value().to_owned(),
        sample,
        held,
        other,
        correctable: review.correctable && !review.suggestion,
        confirm: confirm.map(|(_, reason)| reason),
    };
    respond(state, Some(session), section, StatusCode::OK, page.render())
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Decision {
    Resend,
    Rerun,
    Correction,
    Supersede,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct DecisionForm {
    csrf: String,
    snapshot: String,
    decision: Decision,
    reason: String,
    /// `yes` only from the correction confirmation drawn over the comparison.
    #[serde(default)]
    confirm: String,
}

pub(in crate::admin) async fn decide(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<DecisionForm>,
) -> Response {
    let section = Section::NeedsReview(Some(id));
    let Some(actor) = session.actor().cloned() else {
        return Problem::SignIn.page(&state, None, section);
    };
    if !session.csrf_valid(&form.csrf) {
        return Problem::FormExpired.page(&state, Some(&session), section);
    }
    let decision = match form.decision {
        Decision::Resend => MeasurementDecision::Resend,
        Decision::Rerun => MeasurementDecision::Rerun,
        Decision::Correction => MeasurementDecision::Correction,
        Decision::Supersede => MeasurementDecision::Supersede,
    };
    // The lab app will replace a value: ask before recording a correction.
    if matches!(decision, MeasurementDecision::Correction) && form.confirm != "yes" {
        return compared(&state, id, &session, Some((&form.snapshot, form.reason))).await;
    }
    let result: Result<(), crate::admin::Error> = async {
        let program = if matches!(
            decision,
            MeasurementDecision::Rerun | MeasurementDecision::Correction
        ) {
            Some(
                super::program_for_store(state.setup.config().to_owned(), state.directory.clone())
                    .await?,
            )
        } else {
            None
        };
        measurement::decide(
            &state.store,
            program.as_ref(),
            id,
            form.snapshot,
            decision,
            actor,
            &form.reason,
        )
        .await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Redirect::to(&format!("/parked/{id}")).into_response(),
        Err(error) => Problem::Failed(error.to_string()).page(&state, Some(&session), section),
    }
}
