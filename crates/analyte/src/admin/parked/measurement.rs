//! Shows the audited comparison and accepts the signed-in operator's explicit decision.
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
    let Some(actor) = session.actor().cloned() else {
        return Problem::SignIn.page(&state, None, section);
    };
    let review = match measurement::review(&state.store, id, actor).await {
        Ok(Some(review)) => review,
        Ok(None) => {
            return Problem::Failed(
                tr("admin-no-eligible-measurement-comparison-is-available").into(),
            )
            .page(&state, Some(session), section);
        }
        Err(error) => {
            return Problem::Failed(error.to_string()).page(&state, Some(session), section);
        }
    };
    let (Ok(Some(held)), Ok(Some(other))) = (
        Evidence::from_results(&review.held),
        Evidence::from_results(&review.other),
    ) else {
        return Problem::Failed(tr("admin-the-retained-results-could-not-be-displayed").into())
            .page(&state, Some(session), section);
    };
    let sample = if review.suggestion {
        other.sample.clone()
    } else {
        None
    };
    let page = Page {
        frame: Frame::new(
            &state,
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
    };
    respond(
        &state,
        Some(session),
        section,
        StatusCode::OK,
        page.render(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Decision {
    Resend,
    Rerun,
    Supersede,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct DecisionForm {
    csrf: String,
    snapshot: String,
    decision: Decision,
    reason: String,
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
        Decision::Supersede => MeasurementDecision::Supersede,
    };
    let result: Result<(), crate::admin::Error> = async {
        let program = if matches!(decision, MeasurementDecision::Rerun) {
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
