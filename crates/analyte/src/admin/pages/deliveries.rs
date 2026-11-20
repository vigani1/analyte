//! Serves Deliveries (GET /deliveries) with its All and Not delivered filters, and the audited Send again: its confirmation drawn over the list (GET /deliveries/{id}/resend) and the action (POST), which keeps the same delivery ID and bytes.
//!
//! No script: the dialog is the list with a `.scrim`, its Cancel is a link
//! back to the list, and the form posts with the session's form token. The
//! shared request guard already refused a post without a session or with a
//! wrong token; the action checks again and asks for a signed-in session.
//! Sending again never sends an order to a machine: it only queues the kept
//! delivery for the lab app again (safety rules 4 and 8).
use super::page::{Deliveries, DeliveryRow, Empty, Look, ResendDialog};
use super::{Outcome, PAGE, cursor_value, next_href, one_page, report};
use crate::admin::error::CommandError;
use crate::admin::parked::checked;
use crate::admin::sessions::{Session, signed_in};
use crate::admin::view::{
    Tab, Tone, format,
    frame::Frame,
    report::{Chip, Problem, Section, respond},
};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_model::DeliveryId;
use analyte_store::{
    DeliveryCounts, DeliveryFilter, DeliveryKind, DeliveryState, ListedDelivery, RecentOutcome,
    StoredDelivery,
};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

/// Which deliveries the list shows; the sidebar's link opens them all.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Show {
    #[default]
    All,
    /// Every delivery the lab app has not accepted yet.
    Undelivered,
}

impl Show {
    const fn filter(self) -> DeliveryFilter {
        match self {
            Self::All => DeliveryFilter::All,
            Self::Undelivered => DeliveryFilter::Undelivered,
        }
    }

    const fn href(self) -> &'static str {
        match self {
            Self::All => "/deliveries",
            Self::Undelivered => "/deliveries?show=undelivered",
        }
    }
}

/// GET /deliveries's query: the filter and the cursor of the page.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct DeliveriesQuery {
    #[serde(default)]
    show: Show,
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
}

/// What the list reads in one store request.
struct Read {
    counts: DeliveryCounts,
    rows: Vec<ListedDelivery>,
    more: bool,
}

pub(in crate::admin) async fn deliveries(
    State(state): State<Arc<Context>>,
    Query(query): Query<DeliveriesQuery>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let show = query.show;
    super::show(
        &state,
        session.as_ref(),
        Tab::Deliveries,
        read(&state, show, query.after),
        |read, session, now| list(&state, session, show, &read, None, now).render(),
    )
    .await
}

async fn read(state: &Context, show: Show, after: u64) -> Result<Read, Error> {
    let after = i64::try_from(after)?;
    let filter = show.filter();
    Ok(state
        .store
        .request::<_, CommandError>(deadline()?, move |s| {
            let (rows, more) =
                one_page::<_, CommandError>(s.listed_deliveries(after, filter)?, PAGE, |last| {
                    Ok(!s
                        .listed_deliveries(last.delivery.sequence, filter)?
                        .is_empty())
                })?;
            Ok(Read {
                counts: s.delivery_counts()?,
                rows,
                more,
            })
        })
        .await?)
}

fn list(
    state: &Context,
    session: Option<&Session>,
    show: Show,
    read: &Read,
    dialog: Option<ResendDialog>,
    now: Timestamp,
) -> Deliveries {
    let chips = [
        (Show::All, "All", read.counts.all),
        (Show::Undelivered, "Not delivered", read.counts.undelivered),
    ]
    .into_iter()
    .map(|(filter, label, count)| Chip {
        href: filter.href(),
        label,
        count,
        current: filter == show,
    })
    .collect();
    let shown = match show {
        Show::All => read.counts.all,
        Show::Undelivered => read.counts.undelivered,
    };
    Deliveries {
        frame: Frame::new(
            state,
            Tab::Deliveries,
            Tab::Deliveries.label(),
            None,
            session,
            now,
        ),
        chips,
        empty: read.rows.is_empty().then(|| empty(show, shown)),
        next: read
            .rows
            .last()
            .filter(|_| read.more)
            .map(|last| next_href(show.href(), last.delivery.sequence)),
        rows: read.rows.iter().map(|row| delivery_row(row, now)).collect(),
        dialog,
    }
}

/// What a filter with nothing on this page says.
const fn empty(show: Show, shown: u64) -> Empty {
    if shown > 0 {
        return Empty {
            title: "No more deliveries",
            text: "This page is past the last delivery.",
            link: Some((Show::All.href(), "Back to the first page")),
        };
    }
    match show {
        Show::All => Empty {
            title: "No deliveries yet",
            text: "Results and order messages Analyte sends to a lab app are listed here.",
            link: None,
        },
        Show::Undelivered => Empty {
            title: "Nothing waits for a lab app",
            text: "Every delivery listed was accepted by its lab app.",
            link: Some((Show::All.href(), "Show all deliveries")),
        },
    }
}

/// One delivery's row: what it carries, from which machine to which lab app,
/// and how it stands in the existing delivery words; its ID stays on hover.
fn delivery_row(row: &ListedDelivery, now: Timestamp) -> DeliveryRow {
    let delivery = &row.delivery;
    let (sample, results) = match &row.reading {
        Some(RecentOutcome::Results { results, samples }) => {
            (format::samples(samples), Some(*results))
        }
        Some(RecentOutcome::Failed) | None => (None, None),
    };
    let id = uuid::Uuid::from(delivery.id);
    DeliveryRow {
        queued: row.queued_at.map(|at| format::moment_unix(at, now)),
        sample,
        kind: match row.kind {
            DeliveryKind::Result => None,
            DeliveryKind::OrderEvent => Some("Order message"),
        },
        from: (!row.machines.is_empty()).then(|| {
            row.machines
                .iter()
                .map(analyte_model::Identifier::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        }),
        to: delivery.destination.as_str().to_owned(),
        results,
        outcome: outcome(delivery),
        resend: resendable(delivery.state).then(|| resend_href(id)),
    }
}

/// How a delivery stands, as every page shows its state; its ID and stored
/// state on hover.
fn outcome(delivery: &StoredDelivery) -> Look {
    let look = format::delivery_look(delivery.state);
    Look {
        tone: look.tone,
        icon: look.icon,
        word: look.word,
        exact: format!(
            "Delivery {} · {}",
            uuid::Uuid::from(delivery.id),
            delivery.state
        ),
        note: None,
    }
}

/// Whether the page offers Send again for a delivery in `state`; the store
/// checks again when it runs.
const fn resendable(state: DeliveryState) -> bool {
    match state {
        DeliveryState::Done | DeliveryState::Parked | DeliveryState::Paused => true,
        DeliveryState::Pending | DeliveryState::InFlight | DeliveryState::Conflict => false,
    }
}

/// The Send again dialog's address for delivery `id`.
fn resend_href(id: uuid::Uuid) -> String {
    format!("/deliveries/{id}/resend")
}

fn delivery_id(text: &str) -> Result<DeliveryId, Error> {
    Ok(uuid::Uuid::parse_str(text)?.try_into()?)
}

/// The form the Send again dialog posts; its fields are unchanged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Confirmation {
    csrf: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    confirm: String,
    #[serde(default)]
    acknowledge_superseded: bool,
}

pub(in crate::admin) async fn confirm_resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(session) = signed_in(session.as_ref()) else {
        return Problem::SignIn.page(&state, None, Section::More(Tab::Deliveries));
    };
    open(&state, session, &text, "", false, None).await
}

/// The first page of the list under the Send again dialog: OK on a first look,
/// 400 with `error` when a sent reason was refused; a delivery that is not
/// found is refused instead.
async fn open(
    state: &Context,
    session: &Session,
    text: &str,
    typed: &str,
    superseded: bool,
    error: Option<&'static str>,
) -> Response {
    let section = Section::More(Tab::Deliveries);
    let id = match delivery_id(text) {
        Ok(id) => id,
        Err(error) => {
            return Problem::Refused(error.to_string()).page(state, Some(session), section);
        }
    };
    let both = async {
        let until = deadline()?;
        let delivery = state
            .store
            .request::<_, CommandError>(until, move |s| Ok(s.delivery(id)?))
            .await?;
        Ok::<_, Error>((delivery, read(state, Show::All, 0).await?))
    };
    let (delivery, read) = match tokio::time::timeout(crate::admin::view::READ_BUDGET, both).await {
        Ok(Ok((Some(delivery), read))) => (delivery, read),
        Ok(Ok((None, _))) => {
            return Problem::Refused(analyte_store::DeliveryError::UnknownDelivery.to_string())
                .page(state, Some(session), section);
        }
        Ok(Err(error)) => {
            return Problem::Store(error.to_string()).page(state, Some(session), section);
        }
        Err(_) => return Problem::Time.page(state, Some(session), section),
    };
    let dialog = ResendDialog {
        action: resend_href(uuid::Uuid::from(id)),
        title: "Send this delivery again?",
        text: format!(
            "Analyte sends it to lab app {} again, unchanged and with the same delivery number. Its status now: {}. If the lab app already has it, it treats the repeat as done. Analyte checks again that it may be sent when you confirm.",
            delivery.destination.as_str(),
            format::delivery_look(delivery.state).word
        ),
        placeholder: "For example: the lab app team asked for it again",
        hint: "Required, up to 256 characters. Recorded with the action and the time.",
        button: "Send again",
        csrf: session.csrf.form_value().to_owned(),
        cancel: Tab::Deliveries.href(),
        reason: typed.to_owned(),
        error,
        support: vec![format!("Delivery {}", uuid::Uuid::from(id))],
        superseded,
    };
    let html = list(
        state,
        Some(session),
        Show::All,
        &read,
        Some(dialog),
        Timestamp::now(),
    )
    .render();
    let status = if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    respond(state, Some(session), section, status, html)
}

pub(in crate::admin) async fn resend(
    State(state): State<Arc<Context>>,
    Path(text): Path<String>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<Confirmation>,
) -> Response {
    let section = Section::More(Tab::Deliveries);
    if !(session.authenticated && session.csrf_valid(&form.csrf) && form.confirm == "yes") {
        return Problem::SignIn.page(&state, None, section);
    }
    let reason = match checked(&form.reason) {
        Ok(reason) => reason,
        Err(error) => {
            return open(
                &state,
                &session,
                &text,
                &form.reason,
                form.acknowledge_superseded,
                Some(error),
            )
            .await;
        }
    };
    let id = match delivery_id(&text) {
        Ok(id) => id,
        Err(error) => {
            return Problem::Refused(error.to_string()).page(&state, Some(&session), section);
        }
    };
    match crate::operator::recovery::resend(
        &state.store,
        id,
        form.acknowledge_superseded,
        session.actor.clone(),
        reason,
    )
    .await
    {
        Ok(()) => report(
            &state,
            &session,
            Tab::Deliveries,
            StatusCode::OK,
            Outcome {
                title: "Queued to send again",
                tone: Tone::Good,
                alert: "The delivery is queued to send again".into(),
                text: concat!(
                    "It goes to the lab app unchanged, with the same delivery number, so the ",
                    "lab app treats a repeat as done.",
                )
                .into(),
            },
            vec![format!("Delivery {}", uuid::Uuid::from(id))],
        ),
        Err(error) => Problem::Refused(error.to_string()).page(&state, Some(&session), section),
    }
}
