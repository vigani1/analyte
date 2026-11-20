//! Serves what a machine did with orders, kept apart from the order's own state: Cancelled by machine (GET /analyzer-cancelled) and one order item's page with its history (GET /awos/{order}/{item}).
use super::orders::{look, model, tests};
use super::page::{Cancelled, CancelledCard, ITEM_LEDE, Message, OrderItem};
use super::{Cursor, PAGE, next_href, one_page, show};
use crate::admin::error::CommandError;
use crate::admin::sessions::Session;
use crate::admin::view::{
    Tab, Tone, format,
    format::Line,
    frame::Frame,
    report::{Crumb, Fact, Head, Status, Value},
};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_model::AwosExecutionProjection;
use analyte_store::{CachedOrder, MachineOrder, MachineOrderItem, law::WorkEvidence};
use askama::Template as _;
use axum::{
    Extension,
    extract::{Path, Query, State},
    response::Response,
};
use jiff::Timestamp;
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
        .request::<_, CommandError>(deadline()?, move |s| {
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
    show(
        &state,
        session.as_ref(),
        Tab::MachineOrders,
        read_detail(&state, order, item, cursor),
        |read, session, now| detail_page(&state, session, read, now).render(),
    )
    .await
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

async fn read_detail(
    state: &Context,
    order: i64,
    item: u16,
    cursor: Cursor,
) -> Result<DetailRead, Error> {
    let after = i64::try_from(cursor.after)?;
    let (order, cached, (messages, more)) = state
        .store
        .request::<_, CommandError>(deadline()?, move |s| {
            let order = s.machine_order_by_id(order)?;
            let cached = s.cached_order(&order.key.destination, &order.key.order)?;
            let id = order.id;
            let messages =
                one_page::<_, CommandError>(s.law_work_evidence(id, item, after)?, PAGE, |last| {
                    Ok(!s.law_work_evidence(id, item, last.cursor)?.is_empty())
                })?;
            Ok((order, cached, messages))
        })
        .await?;
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
        messages: messages
            .iter()
            .map(|message| message_of(message, order.id, index, now))
            .collect(),
        next,
        support,
    }
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
    Value::Status(Status {
        tone: match projection.state {
            analyte_model::AwosExecution::Scheduled | analyte_model::AwosExecution::InProgress => {
                Tone::Neutral
            }
            analyte_model::AwosExecution::AnalyzerCancelled => Tone::Attention,
        },
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

/// One message from the machine about this item: who recorded it and how it
/// was applied, what the machine reported and the times it sent, as sent.
fn message_of(message: &WorkEvidence, order: i64, item: u16, now: Timestamp) -> Message {
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
        .unwrap_or("not known");
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
