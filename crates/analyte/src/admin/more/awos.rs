//! Serves what a machine did with orders, kept apart from the order's own state: Cancelled by machine (GET /analyzer-cancelled) and one order item's page with its history (GET /awos/{order}/{item}), whose machine notes show only after the audited "Show the original message" (POST /awos/{order}/{item}/raw).
use super::orders::{look, model, tests};
use super::page::{Cancelled, CancelledCard, ITEM_LEDE, Message, Notes, OrderItem, Original};
use super::{next_href, show};
use crate::admin::error::CommandError;
use crate::admin::parked::{RAW_VIEW, RawRequest};
use crate::admin::sessions::Session;
use crate::admin::view::list::{Cursor, PAGE, cursor_value, one_page};
use crate::admin::view::{
    Tab, Tone, format,
    format::Line,
    frame::Frame,
    report::{Crumb, Fact, Head, Problem, Section, Status, Value, respond},
};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_model::{AwosExecutionProjection, Identifier, Reason};
use analyte_store::{Audit, CachedOrder, MachineOrder, MachineOrderItem, Store, law::WorkEvidence};
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

pub(in crate::admin) async fn cancelled(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    show(
        &state,
        session.as_ref(),
        Tab::CancelledByMachine,
        read_cancelled(&state, cursor),
        |(rows, more), session, now| cancelled_page(&state, session, &rows, more, now).render(),
    )
    .await
}

/// One cancelled item with the lab app's copy of its order, when one is kept.
type CancelledRow = (
    analyte_store::law::AnalyzerCancelledItem,
    Option<CachedOrder>,
);

async fn read_cancelled(
    state: &Context,
    cursor: Cursor,
) -> Result<(Vec<CancelledRow>, bool), Error> {
    let after = i64::try_from(cursor.after)?;
    Ok(state
        .store
        .read::<_, CommandError>(deadline()?, move |s| {
            // This page shows every row of the store's read, not the admin page size.
            let (rows, more) = one_page::<_, CommandError>(
                s.analyzer_cancelled_items(after)?,
                usize::MAX,
                |last| Ok(!s.analyzer_cancelled_items(last.cursor)?.is_empty()),
            )?;
            let rows = rows
                .into_iter()
                .map(|row| {
                    let cached =
                        s.cached_order(&row.order.key.destination, &row.order.key.order)?;
                    Ok::<_, CommandError>((row, cached))
                })
                .collect::<Result<_, _>>()?;
            Ok((rows, more))
        })
        .await?)
}

fn cancelled_page(
    state: &Context,
    session: Option<&Session>,
    rows: &[CancelledRow],
    more: bool,
    now: Timestamp,
) -> Cancelled {
    let next = rows
        .last()
        .filter(|_| more)
        .map(|(last, _)| next_href(Tab::CancelledByMachine.href(), last.cursor));
    Cancelled {
        frame: Frame::new(
            state,
            Tab::CancelledByMachine,
            Tab::CancelledByMachine.label(),
            None,
            session,
            now,
        ),
        cards: rows
            .iter()
            .map(|(row, cached)| card(state, &row.order, &row.item, cached.as_ref(), now))
            .collect(),
        next,
    }
}

/// One cancelled item: its sample from the lab app's copy, its machine and
/// test count, when the machine's cancel was recorded and the lab app's own
/// status as its copy says.
fn card(
    state: &Context,
    order: &MachineOrder,
    item: &MachineOrderItem,
    cached: Option<&CachedOrder>,
    now: Timestamp,
) -> CancelledCard {
    let mut lines = Vec::new();
    if let Some(execution) = &item.execution {
        lines.push(
            Line::text("Cancelled by the machine · recorded ")
                .time(format::moment_unix(execution.recorded_at, now)),
        );
    }
    lines.push(Line::text(match cached.map(lab_app_state) {
        Some(Some(state)) => format!("The lab app's copy says: {state}"),
        Some(None) => "The lab app's copy cannot be read.".to_owned(),
        None => "No copy of this order from the lab app is kept.".to_owned(),
    }));
    CancelledCard {
        sample: cached.map(|cached| cached.sample.as_str().to_owned()),
        order: order.key.order.as_str().to_owned(),
        machine: order.key.machine.as_str().to_owned(),
        model: model(state, order.key.machine.as_str()),
        tests: item
            .coverage_known
            .then(|| format::test_count(item.tests.len())),
        exact: identity(order, item),
        lines,
        href: format::order_item_href(order.id, item.index),
    }
}

/// The lab app's own order status as its kept copy says, in its spelling;
/// none when the copy cannot be read.
fn lab_app_state(cached: &CachedOrder) -> Option<String> {
    let document: serde_json::Value = serde_json::from_str(&cached.document).ok()?;
    Some(
        document
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("not known")
            .to_owned(),
    )
}

/// An order item's identity for the hover: its lab app, the version sent and
/// Analyte's own numbers.
fn identity(order: &MachineOrder, item: &MachineOrderItem) -> String {
    format!(
        "Lab app {} · order version {} sent · machine order {}, item {}",
        order.key.destination.as_str(),
        order.version,
        order.id,
        item.index
    )
}

pub(in crate::admin) async fn detail(
    State(state): State<Arc<Context>>,
    Path((order, item)): Path<(i64, u16)>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let after = cursor.after;
    show(
        &state,
        session.as_ref(),
        Tab::MachineOrders,
        read_detail(&state, order, item, cursor),
        |read, session, now| {
            detail_page(&state, session, read, Reveal::Hidden { after }, now).render()
        },
    )
    .await
}

/// Whether an order item's page shows the machine's notes: hidden behind
/// the audited form for the page of messages after `after`, or shown once
/// the view was recorded.
#[derive(Clone, Copy)]
enum Reveal {
    Hidden { after: u64 },
    Shown,
}

/// What "Show the original message" posts on an order item's page: the
/// fields of Needs review's form ([`RawRequest`]), for one page of messages.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct NotesForm {
    csrf: String,
    target: String,
    source: String,
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
    #[serde(default, deserialize_with = "cursor_value")]
    offset: u64,
    #[serde(default)]
    reason: String,
}

/// The form's `target` and `source`: the machine's notes, which are shown
/// whole, so the form's offset is always 0.
const NOTES: &str = "notes";

/// POST /awos/{order}/{item}/raw: records in the audit who asked to see the
/// machine's notes on this page of messages and why, then shows the page
/// with them. Nothing shows unless that record was stored; a page with no
/// notes records nothing.
pub(in crate::admin) async fn notes(
    State(state): State<Arc<Context>>,
    Path((order, item)): Path<(i64, u16)>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<NotesForm>,
) -> Response {
    let section = Section::More(Tab::MachineOrders);
    if !session.authenticated {
        return Problem::SignIn.page(&state, None, section);
    }
    if !session.csrf_valid(&form.csrf)
        || form.target != NOTES
        || form.source != NOTES
        || form.offset != 0
    {
        return Problem::Refused(
            "This form is not one Analyte sent for this page. Open the order again and repeat what you did."
                .into(),
        )
        .page(&state, Some(&session), section);
    }
    let after = form.after;
    let read = async {
        let audit = Audit {
            actor: session.actor.clone(),
            action: Identifier::new(RAW_VIEW)?,
            targets: vec![
                Identifier::new(&format!("awos:{order}:{item}"))?,
                Identifier::new(NOTES)?,
                Identifier::new(&format!("notes-after:{after}"))?,
            ],
            time: Timestamp::now().as_second(),
            reason: Reason::new(if form.reason.is_empty() {
                "operator requested the machine's notes"
            } else {
                &form.reason
            })?,
        };
        let after = i64::try_from(after)?;
        let read = state
            .store
            .request::<_, CommandError>(deadline()?, move |s| {
                let read = read_in(s, order, item, after)?;
                let id = read.order.id;
                if read
                    .messages
                    .iter()
                    .any(|message| !notes_of(message, id, item).is_empty())
                {
                    s.audit(&audit)?;
                }
                Ok(read)
            })
            .await?;
        settle(read, item)
    }
    .await;
    match read {
        Ok(read) => respond(
            &state,
            Some(&session),
            section,
            StatusCode::OK,
            detail_page(
                &state,
                Some(&session),
                read,
                Reveal::Shown,
                Timestamp::now(),
            )
            .render(),
        ),
        Err(error) => Problem::of(&error).page(&state, Some(&session), section),
    }
}

/// What an order item's page reads in one store request.
struct DetailRead {
    order: MachineOrder,
    item: MachineOrderItem,
    cached: Option<CachedOrder>,
    messages: Vec<WorkEvidence>,
    /// More messages follow this page.
    more: bool,
}

/// What [`read_in`] reads, before its item is found among the order's.
struct Stored {
    order: MachineOrder,
    cached: Option<CachedOrder>,
    messages: Vec<WorkEvidence>,
    more: bool,
}

async fn read_detail(
    state: &Context,
    order: i64,
    item: u16,
    cursor: Cursor,
) -> Result<DetailRead, Error> {
    let after = i64::try_from(cursor.after)?;
    let read = state
        .store
        .read::<_, CommandError>(deadline()?, move |s| read_in(s, order, item, after))
        .await?;
    settle(read, item)
}

/// The order, the lab app's copy and one page of the machine's messages
/// about `item` after `after`, within the caller's store request.
fn read_in(s: &Store, order: i64, item: u16, after: i64) -> Result<Stored, CommandError> {
    let order = s.machine_order_by_id(order)?;
    let cached = s.cached_order(&order.key.destination, &order.key.order)?;
    let id = order.id;
    let (messages, more) =
        one_page::<_, CommandError>(s.law_work_evidence(id, item, after)?, PAGE, |last| {
            Ok(!s.law_work_evidence(id, item, last.cursor)?.is_empty())
        })?;
    Ok(Stored {
        order,
        cached,
        messages,
        more,
    })
}

/// The read with its item, which the order must still hold.
fn settle(read: Stored, item: u16) -> Result<DetailRead, Error> {
    let Stored {
        order,
        cached,
        messages,
        more,
    } = read;
    let item = order
        .items
        .iter()
        .find(|kept| kept.index == item)
        .cloned()
        .ok_or(Error::AwosItemNotRetained)?;
    Ok(DetailRead {
        order,
        item,
        cached,
        messages,
        more,
    })
}

fn detail_page(
    state: &Context,
    session: Option<&Session>,
    read: DetailRead,
    reveal: Reveal,
    now: Timestamp,
) -> OrderItem {
    let DetailRead {
        order,
        item,
        cached,
        messages,
        more,
    } = read;
    let index = item.index;
    let number = order.key.order.as_str();
    let title = if order.items.len() > 1 {
        format!("Order {number}, item {index}")
    } else {
        format!("Order {number}")
    };
    let status = look(item.state, &item.source_code);
    let (copy, copy_note) = lab_app_copy(&order, cached.as_ref(), now);
    let next = messages
        .last()
        .filter(|_| more)
        .map(|last| next_href(&format::order_item_href(order.id, index), last.cursor));
    let mut support = vec![identity(&order, &item)];
    support.extend(messages.iter().map(|message| {
        format!(
            "Message {} · receipt {}, reading {}",
            message.custody.message.as_str(),
            uuid::Uuid::from(message.receipt),
            message.version
        )
    }));
    let messages: Vec<Message> = messages
        .iter()
        .map(|message| message_of(message, order.id, index, reveal, now))
        .collect();
    let shown = messages
        .iter()
        .any(|message| matches!(message.notes, Notes::Shown(_)));
    let original = original(session, reveal, &messages, order.id, index);
    OrderItem {
        frame: Frame::new(state, Tab::MachineOrders, title.clone(), None, session, now),
        head: Head {
            crumbs: vec![
                Crumb {
                    label: Tab::MachineOrders.label().to_owned(),
                    href: Some(Tab::MachineOrders.href().to_owned()),
                },
                Crumb {
                    label: format!("Order {number}"),
                    href: None,
                },
            ],
            title,
            status: Some(Status {
                tone: status.tone,
                icon: status.icon,
                word: status.word.to_owned(),
                exact: status.exact,
                note: None,
            }),
            lede: Some(Line::text(ITEM_LEDE)),
        },
        order: order_facts(state, &order, cached.as_ref(), now),
        item: item_facts(&item, now),
        others: order
            .items
            .iter()
            .filter(|other| other.index != index)
            .map(|other| {
                (
                    format::order_item_href(order.id, other.index),
                    format!("Item {}", other.index),
                )
            })
            .collect(),
        copy,
        copy_note,
        messages,
        original,
        shown,
        next,
        support,
    }
}

/// "Show the original message" while `messages` hold hidden notes: the
/// audited form for the page of messages it shows, or none for the admin key
/// alone, which has no session to record the view with.
fn original(
    session: Option<&Session>,
    reveal: Reveal,
    messages: &[Message],
    order: i64,
    item: u16,
) -> Option<Original> {
    let Reveal::Hidden { after } = reveal else {
        return None;
    };
    messages
        .iter()
        .any(|message| matches!(message.notes, Notes::Hidden(_)))
        .then(|| Original {
            form: session
                .filter(|session| session.authenticated)
                .map(|session| RawRequest {
                    action: format!("{}/raw", format::order_item_href(order, item)),
                    csrf: session.csrf.form_value().to_owned(),
                    target: NOTES.to_owned(),
                    after,
                    offset: 0,
                    field: "notes-reason".into(),
                    reason: String::new(),
                    buttons: vec![(NOTES, "Show the notes")],
                }),
        })
}

/// The order as a whole: its machine, lab app, state, the version sent, its
/// sample from the lab app's copy, how many tests it sent and its last change.
fn order_facts(
    state: &Context,
    order: &MachineOrder,
    cached: Option<&CachedOrder>,
    now: Timestamp,
) -> Vec<Fact> {
    let machine = order.key.machine.as_str();
    let order_status = look(order.state, &order.source_code);
    let mut facts = vec![
        Fact {
            term: "Machine",
            value: Value::Page {
                href: format::machine_href(machine),
                label: match model(state, machine) {
                    Some(model) => format!("{machine} ({model})"),
                    None => machine.to_owned(),
                },
            },
        },
        Fact {
            term: "Lab app",
            value: Value::Page {
                href: format::lab_app_href(order.key.destination.as_str()),
                label: order.key.destination.as_str().to_owned(),
            },
        },
        Fact {
            term: "Order status",
            value: Value::Status(Status {
                tone: order_status.tone,
                icon: order_status.icon,
                word: order_status.word.to_owned(),
                exact: order_status.exact,
                note: order_status.note,
            }),
        },
        Fact {
            term: "Order version sent",
            value: Value::Text(order.version.to_string()),
        },
    ];
    if let Some(cached) = cached {
        facts.push(Fact {
            term: "Sample",
            value: Value::Code {
                shown: cached.sample.as_str().to_owned(),
                exact: cached.sample.as_str().to_owned(),
            },
        });
    }
    facts.push(Fact {
        term: "Tests sent",
        value: Value::Text(tests(order).map_or_else(
            || "Not recorded (older data); Analyte does not guess them".to_owned(),
            format::test_count,
        )),
    });
    facts.push(Fact {
        term: "Last change",
        value: Value::Time(Some(format::moment_unix(order.updated_at, now))),
    });
    facts
}

/// This item: its state, what the machine reports, how many tests it covers,
/// the number Analyte gave it on the machine and the last note recorded.
fn item_facts(item: &MachineOrderItem, now: Timestamp) -> Vec<Fact> {
    let status = look(item.state, &item.source_code);
    vec![
        Fact {
            term: "Status",
            value: Value::Status(Status {
                tone: status.tone,
                icon: status.icon,
                word: status.word.to_owned(),
                exact: status.exact,
                note: None,
            }),
        },
        Fact {
            term: "What the machine reports",
            value: execution(item.execution.as_ref(), now),
        },
        Fact {
            term: "Tests",
            value: Value::Text(if item.coverage_known {
                format::amount(item.tests.len() as u64, "test", "tests")
            } else {
                "Not recorded (older data)".to_owned()
            }),
        },
        Fact {
            term: "Order number on the machine",
            value: Value::Code {
                shown: item.placer.clone(),
                exact: item.placer_namespace.as_ref().map_or_else(
                    || "Namespace not recorded".to_owned(),
                    |namespace| format!("Namespace {}", namespace.as_str()),
                ),
            },
        },
        Fact {
            term: "Last note",
            value: Value::Text(item.source_code.as_str().to_owned()),
        },
    ]
}

/// What the machine last reported doing with an item, kept apart from the
/// item's own state: its words, when Analyte recorded it and whether a cancel
/// request was pending then; the message it came in stays on hover.
fn execution(projection: Option<&AwosExecutionProjection>, now: Timestamp) -> Value {
    let Some(projection) = projection else {
        return Value::Text("Nothing yet".into());
    };
    let recorded = format::moment_unix(projection.recorded_at, now);
    let tone = match projection.state {
        analyte_model::AwosExecution::Scheduled | analyte_model::AwosExecution::InProgress => {
            Tone::Neutral
        }
        analyte_model::AwosExecution::AnalyzerCancelled => Tone::Attention,
    };
    Value::Status(Status {
        tone,
        icon: tone.icon(),
        word: format::execution(projection.state).to_owned(),
        exact: format!(
            "Receipt {}, reading {}",
            uuid::Uuid::from(projection.receipt),
            projection.interpretation
        ),
        note: Some(format!(
            "Recorded {}{}",
            recorded.shown,
            if projection.cancel_outstanding {
                "; a cancel request was pending then"
            } else {
                ""
            }
        )),
    })
}

/// The lab app's kept copy of the order: its sample, whether the lab app
/// cancelled it, how long it is kept and whether it is the version sent; or
/// why it is not shown. Its tests never show.
fn lab_app_copy(
    order: &MachineOrder,
    cached: Option<&CachedOrder>,
    now: Timestamp,
) -> (Vec<Fact>, Option<String>) {
    let Some(cached) = cached else {
        return (
            Vec::new(),
            Some("No copy of this order from the lab app is kept.".into()),
        );
    };
    let mut facts = vec![
        Fact {
            term: "Cancelled in the lab app",
            value: Value::Text(format::yes_no(cached.tombstone).into()),
        },
        Fact {
            term: "Kept until",
            value: Value::Time(Some(format::moment_unix(cached.expiry, now))),
        },
    ];
    let note = match analyte_contract::decode::<analyte_contract::Order>(cached.document.as_bytes())
    {
        Ok(document) => {
            let version = document.fields().version;
            facts.push(Fact {
                    term: "Its version",
                    value: Value::Text(if version == order.version {
                        format!("{version}: the same version that was sent")
                    } else {
                        format!(
                            "{version}: a different version from the one sent, so its tests are not the ones the machine got"
                        )
                    }),
                });
            None
        }
        Err(error) => Some(format!(
            "The lab app's copy cannot be read, so its version is not shown. Details for support: {error}"
        )),
    };
    (facts, note)
}

/// The free text `message` carries about `item` of `order`: the machine's
/// notes, or its source as stored when Analyte cannot read it.
fn notes_of(message: &WorkEvidence, order: i64, item: u16) -> Vec<String> {
    message
        .custody
        .work
        .iter()
        .filter(|work| work.order == order && work.item == item)
        .flat_map(|work| source_evidence(work.source.as_str()).1)
        .collect()
}

/// One message from the machine about this item: who recorded it and how it
/// was applied, what the machine reported and the times it sent, as sent;
/// its notes as `reveal` says.
fn message_of(
    message: &WorkEvidence,
    order: i64,
    item: u16,
    reveal: Reveal,
    now: Timestamp,
) -> Message {
    let mut facts = vec![
        Fact {
            term: "Recorded by Analyte",
            value: Value::Time(Some(format::moment_unix(message.custody.recorded_at, now))),
        },
        Fact {
            term: "Outcome",
            value: Value::Text(message.disposition.to_string()),
        },
    ];
    let mut notes = Vec::new();
    for work in message
        .custody
        .work
        .iter()
        .filter(|work| work.order == order && work.item == item)
    {
        facts.push(Fact {
            term: "What the machine reports",
            value: Value::Text(
                work.execution
                    .map_or("Nothing", format::execution)
                    .to_owned(),
            ),
        });
        facts.push(Fact {
            term: "Results came with it",
            value: Value::Text(format::yes_no(work.complete).into()),
        });
        let (times, written) = source_evidence(work.source.as_str());
        facts.extend(times);
        notes.extend(written);
    }
    let notes = match reveal {
        _ if notes.is_empty() => Notes::None,
        Reveal::Hidden { .. } => Notes::Hidden(format!(
            "{} from the machine, hidden",
            format::amount(
                u64::try_from(notes.len()).unwrap_or(u64::MAX),
                "note",
                "notes"
            )
        )),
        Reveal::Shown => Notes::Shown(notes),
    };
    Message {
        title: format!(
            "Message received {}",
            message
                .received_at
                .as_str()
                .parse::<Timestamp>()
                .map_or_else(
                    |_| message.received_at.as_str().to_owned(),
                    |at| format::clock(at, now)
                )
        ),
        facts,
        notes,
    }
}

/// The times the machine sent with its message, as sent, and its notes; a
/// source Analyte cannot read is shown as one note, unchanged.
fn source_evidence(source: &str) -> (Vec<Fact>, Vec<String>) {
    let unread = || (Vec::new(), vec![source.to_owned()]);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
        return unread();
    };
    let control = value
        .get("control")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let Ok(control) = analyte_hl7::law::RetainedControl::parse(control) else {
        return unread();
    };
    let estimate = match control.estimate() {
        "" => "Not sent; the earlier expected time is kept",
        "\"\"" => "Sent as empty (cleared by the machine)",
        supplied => supplied,
    };
    let header = value
        .get("header_time")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(format::NOT_RECORDED);
    let event = match control.event_time() {
        "" => "not sent",
        supplied => supplied,
    };
    let facts = vec![
        Fact {
            term: "Expected ready time, as sent",
            value: Value::Text(estimate.to_owned()),
        },
        Fact {
            term: "Message time and zone",
            value: Value::Text(header.to_owned()),
        },
        Fact {
            term: "Event time, as sent",
            value: Value::Text(event.to_owned()),
        },
    ];
    let notes = value
        .get("notes")
        .and_then(serde_json::Value::as_array)
        .map(|notes| {
            notes
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (facts, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    // ADR 0016
    #[test]
    fn estimates_keep_null_and_absence_and_keep_notes_as_sent() {
        for (estimate, expected) in [
            ("", "Not sent; the earlier expected time is kept"),
            ("\"\"", "Sent as empty (cleared by the machine)"),
            ("20260101", "20260101"),
        ] {
            let mut fields = vec![""; 28];
            fields[0] = "ORC";
            fields[27] = estimate;
            let source = serde_json::json!({
                "control": fields.join("|"),
                "header_time": "20260101120000+0130",
                "notes": ["NTE|||<script>patient</script>"],
            })
            .to_string();
            let (facts, notes) = source_evidence(&source);
            let texts: Vec<&str> = facts
                .iter()
                .filter_map(|fact| match &fact.value {
                    Value::Text(text) => Some(text.as_str()),
                    Value::Code { .. }
                    | Value::Status(_)
                    | Value::Link(_)
                    | Value::Time(_)
                    | Value::Page { .. } => None,
                })
                .collect();
            assert!(texts.contains(&expected), "{texts:?}");
            assert!(texts.contains(&"20260101120000+0130"));
            assert_eq!(notes, ["NTE|||<script>patient</script>"]);
        }
        let (facts, notes) = source_evidence("not json");
        assert!(facts.is_empty());
        assert_eq!(notes, ["not json"]);
    }
}
