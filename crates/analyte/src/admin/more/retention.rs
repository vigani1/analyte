//! Serves Data keeping (GET and POST /retention): how long each machine's messages are kept, the last clean-up, the active holds with who placed them, and the audited hold form, with each hold's Release dialog drawn over the page (GET /retention?release={hold}).
use super::page::{HoldRow, Keeping, ReleaseDialog};
use super::{Outcome, next_href, report, show};
use crate::admin::error::CommandError;
use crate::admin::server::sessions::{Session, signed_in};
use crate::admin::view::list::{PAGE, cursor_value, one_page};
use crate::admin::view::parts::ReasonDialog;
use crate::admin::view::{
    Tab, Tone, format,
    format::Line,
    frame::Frame,
    report::{Fact, Status, Value},
};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_i18n::{count as tr_count, message as tr_args, text as tr};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, HoldTarget, PurgeRun, RetentionHold};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

/// GET /retention's query: the cursor of the holds and the hold to release.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct KeepingQuery {
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
    /// The hold whose Release dialog is drawn over the page.
    #[serde(default)]
    release: Option<i64>,
}

/// What Data keeping reads in one store request.
struct Read {
    last: Option<PurgeRun>,
    oldest: Option<i64>,
    holds: Vec<Held>,
    /// More holds follow this page.
    more: bool,
    /// The hold the Release dialog is about, when one was asked for.
    releasing: Option<Held>,
}

/// One active hold and, for a message, where it came from.
struct Held {
    hold: RetentionHold,
    message: Option<(String, Timestamp)>,
}

pub(in crate::admin) async fn show_page(
    State(state): State<Arc<Context>>,
    Query(query): Query<KeepingQuery>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let signed = signed_in(session.as_ref());
    let release = query.release.filter(|_| signed.is_some());
    show(
        &state,
        session.as_ref(),
        Tab::DataKeeping,
        read(&state.store, query.after, release),
        |read, session, now| page(&state, session, signed, &read, now).render(),
    )
    .await
}

async fn read(
    store: &analyte_store::Handle,
    after: u64,
    release: Option<i64>,
) -> Result<Read, Error> {
    let after = i64::try_from(after)?;
    Ok(store
        .read::<_, CommandError>(deadline()?, move |store| {
            let held = |hold: RetentionHold| -> Result<Held, CommandError> {
                let message = match &hold.target {
                    HoldTarget::Receipt(id) => store.receipt_metadata(*id)?.map(|metadata| {
                        (metadata.machine.as_str().to_owned(), metadata.received_at)
                    }),
                    HoldTarget::Order { .. } => None,
                };
                Ok(Held { hold, message })
            };
            let releasing = match release {
                Some(sequence) => store
                    .retention_holds(sequence.saturating_sub(1))?
                    .into_iter()
                    .find(|hold| hold.sequence == sequence)
                    .map(&held)
                    .transpose()?,
                None => None,
            };
            let (holds, more) =
                one_page::<_, CommandError>(store.retention_holds(after)?, PAGE, |last| {
                    Ok(!store.retention_holds(last.sequence)?.is_empty())
                })?;
            Ok(Read {
                last: store.last_purge_run()?,
                oldest: store.oldest_retained_receipt()?,
                holds: holds.into_iter().map(&held).collect::<Result<_, _>>()?,
                more,
                releasing,
            })
        })
        .await?)
}

fn page(
    state: &Context,
    session: Option<&Session>,
    signed: Option<&Session>,
    read: &Read,
    now: Timestamp,
) -> Keeping {
    let next = read
        .holds
        .last()
        .filter(|_| read.more)
        .map(|last| next_href(Tab::DataKeeping.href(), last.hold.sequence));
    Keeping {
        frame: Frame::new(
            state,
            Tab::DataKeeping,
            Tab::DataKeeping.label(),
            None,
            session,
            now,
        ),
        policies: state
            .retention
            .iter()
            .map(|(machine, days)| (machine.as_str().to_owned(), policy(*days)))
            .collect(),
        clean_up: read.last.as_ref().map(|run| clean_up(run, now)),
        oldest: Fact {
            term: tr("admin-oldest-message-kept"),
            value: Value::Time(read.oldest.map(|time| format::moment_unix(time, now))),
        },
        holds: read
            .holds
            .iter()
            .map(|held| hold_row(held, signed.is_some(), now))
            .collect(),
        next,
        form: signed.map(|session| session.csrf.form_value().to_owned()),
        dialog: signed
            .zip(read.releasing.as_ref())
            .map(|(session, held)| release_dialog(session, held, now)),
    }
}

/// How long a machine's messages are kept, in the page's words.
fn policy(days: Option<u16>) -> String {
    days.map_or_else(
        || tr("admin-kept-never-deleted").to_owned(),
        |days| {
            tr_args(
                "admin-deleted-after-the-lab-app-last-confirmed-it",
                [(
                    "days",
                    (tr_count("admin-count-days", days)).to_string().into(),
                )],
            )
        },
    )
}

/// Data keeping as `analyte data-keeping` prints it, in the page's words.
#[derive(serde::Serialize)]
pub(in crate::admin) struct Printed {
    /// Each machine and how long its messages are kept.
    pub(in crate::admin) kept: Vec<(String, String)>,
    /// What the last clean-up did, term and value; none before the first.
    pub(in crate::admin) clean_up: Option<Vec<(&'static str, String)>>,
    /// When the oldest message kept arrived, or "Nothing yet".
    pub(in crate::admin) oldest: String,
    /// One page of active holds.
    pub(in crate::admin) holds: Vec<PrintedHold>,
    /// The hold number to pass as `--after` for the next page.
    pub(in crate::admin) next: Option<i64>,
}

/// One active hold, as the page lists it.
#[derive(serde::Serialize)]
pub(in crate::admin) struct PrintedHold {
    /// The hold's number, which `analyte holds release` takes.
    pub(in crate::admin) number: i64,
    /// What it keeps.
    pub(in crate::admin) keeps: String,
    /// Who placed it, when and why.
    pub(in crate::admin) placed: String,
    /// Its stored target: the message ID, or the lab app and order number.
    pub(in crate::admin) target: String,
}

/// Data keeping for the command line: `retention` is each machine's policy
/// as the program sets it, and the holds page starts after hold `after`.
/// # Errors
/// Reports deadline overflow, cursor and store read failures.
pub(in crate::admin) async fn printed(
    store: &analyte_store::Handle,
    retention: &[(Identifier, Option<u16>)],
    after: u64,
) -> Result<Printed, Error> {
    let read = read(store, after, None).await?;
    let now = Timestamp::now();
    let facts = |facts: Vec<Fact>| {
        facts
            .into_iter()
            .map(|fact| (fact.term, fact.value.plain()))
            .collect()
    };
    Ok(Printed {
        kept: retention
            .iter()
            .map(|(machine, days)| (machine.as_str().to_owned(), policy(*days)))
            .collect(),
        clean_up: read.last.as_ref().map(|run| facts(clean_up(run, now))),
        oldest: Value::Time(read.oldest.map(|time| format::moment_unix(time, now))).plain(),
        holds: read
            .holds
            .iter()
            .map(|held| PrintedHold {
                number: held.hold.sequence,
                keeps: held_line(held, now).plain(),
                placed: placed(held.hold.placed.as_ref(), now).plain(),
                target: target(&held.hold.target),
            })
            .collect(),
        next: read
            .holds
            .last()
            .filter(|_| read.more)
            .map(|last| last.hold.sequence),
    })
}

/// What the last clean-up did, as plain numbers; a failure shows red.
fn clean_up(run: &PurgeRun, now: Timestamp) -> Vec<Fact> {
    let number = |term, value: usize| Fact {
        term,
        value: Value::Text(value.to_string()),
    };
    vec![
        Fact {
            term: tr("admin-ran"),
            value: Value::Time(Some(format::moment_unix(run.time, now))),
        },
        number(tr("admin-messages-checked"), run.examined),
        number(tr("admin-messages-deleted"), run.purged),
        number(tr("admin-orders-deleted"), run.orders),
        Fact {
            term: tr("admin-paused"),
            value: Value::Text(format::yes_no(run.paused).into()),
        },
        Fact {
            term: tr("admin-failed"),
            value: Value::Status(if run.failed {
                Status {
                    tone: Tone::Problem,
                    icon: Tone::Problem.icon(),
                    word: tr("setup-yes").into(),
                    exact: tr("admin-purge-failed").into(),
                    note: None,
                }
            } else {
                Status {
                    tone: Tone::Good,
                    icon: Tone::Good.icon(),
                    word: tr("setup-no").into(),
                    exact: String::new(),
                    note: None,
                }
            }),
        },
    ]
}

/// What a hold keeps, in words, with when its message arrived.
fn held_line(held: &Held, now: Timestamp) -> Line {
    match (&held.hold.target, &held.message) {
        (HoldTarget::Receipt(_), Some((machine, at))) => Line::text(tr_args(
            "admin-message-from-received",
            [("machine", (machine).to_string().into())],
        ))
        .time(format::moment(*at, now)),
        (HoldTarget::Receipt(_), None) => Line::text(tr("admin-a-message-no-longer-listed")),
        (HoldTarget::Order { destination, order }, _) => Line::text(tr_args(
            "admin-order-from-lab-app",
            [
                ("order", (order.as_str()).to_string().into()),
                ("destination", (destination.as_str()).to_string().into()),
            ],
        )),
    }
}

/// One hold: what it keeps, who placed it, when and why, and Release.
fn hold_row(held: &Held, signed_in: bool, now: Timestamp) -> HoldRow {
    HoldRow {
        title: held_line(held, now),
        sub: placed(held.hold.placed.as_ref(), now),
        exact: target(&held.hold.target),
        release: signed_in.then(|| format!("/retention?release={}", held.hold.sequence)),
    }
}

/// Who placed a hold, when and why, from the audit history; holds never expire.
fn placed(audit: Option<&Audit>, now: Timestamp) -> Line {
    match audit {
        Some(audit) => Line::text(tr_args(
            "admin-set-by",
            [(
                "actor",
                (format::actor(audit.actor.as_str())).to_string().into(),
            )],
        ))
        .then(Line::text("").time(format::moment_unix(audit.time, now)))
        .then(Line::text(tr_args(
            "admin-hold-reason",
            [("reason", audit.reason.as_str().into())],
        )))
        .then(Line::text(tr("admin-until-released"))),
        None => Line::text(tr("admin-who-placed-it-is-not-in-the-history-until")),
    }
}

/// The stored target, for the hover and the form: "receipt" with the message
/// ID, or "order" with the lab app and its order number.
fn target(target: &HoldTarget) -> String {
    match target {
        HoldTarget::Receipt(id) => tr_args(
            "admin-hold-message",
            [("id", uuid::Uuid::from(*id).to_string().into())],
        ),
        HoldTarget::Order { destination, order } => tr_args(
            "admin-order-lab-app",
            [
                ("order", (order.as_str()).to_string().into()),
                ("destination", (destination.as_str()).to_string().into()),
            ],
        ),
    }
}

/// The Release dialog for one hold: the form's own fields, hidden.
fn release_dialog(session: &Session, held: &Held, now: Timestamp) -> ReleaseDialog {
    let (scope, id, destination) = match &held.hold.target {
        HoldTarget::Receipt(id) => ("receipt", uuid::Uuid::from(*id).to_string(), String::new()),
        HoldTarget::Order { destination, order } => (
            "order",
            order.as_str().to_owned(),
            destination.as_str().to_owned(),
        ),
    };
    let form = ReasonDialog {
        action: Tab::DataKeeping.href().into(),
        title: tr("admin-release-this-hold"),
        text: tr_args(
            "admin-clean-up-may-delete-it-once-its-keeping-time",
            [("plain", (held_line(held, now).plain()).to_string().into())],
        ),
        placeholder: tr("admin-for-example-the-audit-is-finished"),
        hint: tr("admin-required-up-to-256-characters-recorded-with-the-time"),
        button: release(),
        csrf: session.csrf.form_value().to_owned(),
        cancel: Tab::DataKeeping.href().into(),
        reason: String::new(),
        error: None,
    };
    ReleaseDialog {
        form,
        hidden: vec![
            ("scope", scope.to_owned()),
            ("id", id),
            ("destination", destination),
            ("operation", "release".to_owned()),
        ],
    }
}

/// The hold form; its fields are unchanged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct HoldForm {
    csrf: String,
    scope: String,
    id: String,
    destination: String,
    operation: String,
    reason: String,
    confirm: String,
}

pub(in crate::admin) async fn hold(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<HoldForm>,
) -> Response {
    let Some(actor) = session
        .actor()
        .filter(|_| session.csrf_valid(&form.csrf) && form.confirm == "yes")
        .cloned()
    else {
        return report(
            &state,
            &session,
            Tab::DataKeeping,
            StatusCode::FORBIDDEN,
            Outcome {
                title: hold_not_saved(),
                tone: Tone::Problem,
                alert: format::nothing_changed().into(),
                text: tr("admin-tick-the-confirmation-box-and-make-sure-you-are").into(),
            },
            vec![tr("admin-authenticated-session-valid-form-and-confirmation-required").into()],
        );
    };
    let release = form.operation == "release";
    match apply(&state, actor, form).await {
        Ok(()) => report(
            &state,
            &session,
            Tab::DataKeeping,
            StatusCode::OK,
            Outcome {
                title: if release {
                    tr("admin-hold-released")
                } else {
                    tr("admin-hold-saved")
                },
                tone: Tone::Good,
                alert: if release {
                    tr("admin-the-hold-is-released").into()
                } else {
                    tr("admin-the-hold-is-saved").into()
                },
                text: tr("admin-your-session-and-reason-are-recorded-with-it").into(),
            },
            Vec::new(),
        ),
        Err(error) => report(
            &state,
            &session,
            Tab::DataKeeping,
            StatusCode::CONFLICT,
            Outcome {
                title: hold_not_saved(),
                tone: Tone::Problem,
                alert: format::nothing_changed().into(),
                text: format::sentence(&error.to_string()),
            },
            Vec::new(),
        ),
    }
}

async fn apply(state: &Context, actor: Identifier, form: HoldForm) -> Result<(), Error> {
    let target = match form.scope.as_str() {
        "receipt" => {
            HoldTarget::Receipt(serde_json::from_value(serde_json::Value::String(form.id))?)
        }
        "order" => HoldTarget::Order {
            destination: Identifier::new(&form.destination)?,
            order: Identifier::new(&form.id)?,
        },
        _ => return Err(Error::ChooseReceiptOrOrder),
    };
    let active = match form.operation.as_str() {
        "place" => true,
        "release" => false,
        _ => return Err(Error::ChoosePlaceOrRelease),
    };
    let audit = Audit {
        actor,
        action: Identifier::new(if active {
            "retention-hold-place"
        } else {
            "retention-hold-release"
        })?,
        targets: Vec::new(),
        time: Timestamp::now().as_second(),
        reason: Reason::new(&form.reason)?,
    };
    state
        .store
        .request(deadline()?, move |store| {
            store.set_retention_hold(&target, active, &audit)
        })
        .await?;
    Ok(())
}

/// Labels the most recent data clean-up.
pub(in crate::admin) fn last_clean_up() -> &'static str {
    tr("admin-last-clean-up")
}

/// Labels the list of retention holds.
pub(in crate::admin) fn holds() -> &'static str {
    tr("admin-holds")
}

/// Heads the report when a hold fails to save.
pub(in crate::admin) fn hold_not_saved() -> &'static str {
    tr("admin-hold-not-saved")
}

/// Labels the action to add a retention hold.
pub(in crate::admin) fn add_hold() -> &'static str {
    tr("admin-add-hold")
}

/// Labels the action to release a retention hold.
pub(in crate::admin) fn release() -> &'static str {
    tr("admin-release")
}
