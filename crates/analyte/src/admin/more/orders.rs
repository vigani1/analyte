//! Serves Machine orders (GET /orders): the All, Unknown and Cancelled filters with their counts, then one row per order Analyte sent to a machine with its four separate states.
use super::next_href;
use super::page::{Look, OrderRow, Orders};
use crate::admin::error::CommandError;
use crate::admin::server::sessions::Session;
use crate::admin::view::list::{Empty, PAGE, cursor_value, one_page};
use crate::admin::view::{Tab, format, frame::Frame, report::Chip};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_i18n::{message as tr_args, text as tr};
use analyte_model::OrderState;
use analyte_store::{CachedOrder, MachineOrder, MachineOrderFilter};
use askama::Template as _;
use axum::{
    Extension,
    extract::{Query, State},
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;

/// Which orders the list shows; the sidebar's link opens them all.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Show {
    #[default]
    All,
    /// Orders whose outcome is unknown: check the machine.
    Unknown,
    /// Orders the lab app cancelled and the machine confirmed.
    Cancelled,
}

impl Show {
    const fn state(self) -> Option<OrderState> {
        match self {
            Self::All => None,
            Self::Unknown => Some(OrderState::Uncertain),
            Self::Cancelled => Some(OrderState::CancelConfirmed),
        }
    }

    const fn href(self) -> &'static str {
        match self {
            Self::All => "/orders",
            Self::Unknown => "/orders?show=unknown",
            Self::Cancelled => "/orders?show=cancelled",
        }
    }
}

/// GET /orders's query: the filter and the cursor of the page.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct OrdersQuery {
    #[serde(default)]
    show: Show,
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
}

/// What GET /orders reads in one store request.
struct Read {
    paused: bool,
    counts: Vec<(OrderState, u64)>,
    orders: Vec<(MachineOrder, Option<CachedOrder>)>,
    more: bool,
}

pub(in crate::admin) async fn orders(
    State(state): State<Arc<Context>>,
    Query(query): Query<OrdersQuery>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let show = query.show;
    super::show(
        &state,
        session.as_ref(),
        Tab::MachineOrders,
        read(&state, query),
        |read, session, now| page(&state, session, show, &read, now).render(),
    )
    .await
}

async fn read(state: &Context, query: OrdersQuery) -> Result<Read, Error> {
    let after = i64::try_from(query.after)?;
    let filter = move |after| MachineOrderFilter {
        after,
        machine: None,
        state: query.show.state(),
    };
    Ok(state
        .store
        .read::<_, CommandError>(deadline()?, move |s| {
            let (orders, more) = one_page::<_, CommandError>(
                s.machine_orders_up_to(&filter(after), PAGE + 1)?,
                PAGE,
                |last| Ok(!s.machine_orders_up_to(&filter(last.id), 1)?.is_empty()),
            )?;
            let orders = orders
                .into_iter()
                .map(|order| {
                    let cached = s.cached_order(&order.key.destination, &order.key.order)?;
                    Ok::<_, CommandError>((order, cached))
                })
                .collect::<Result<_, _>>()?;
            Ok(Read {
                paused: s.restore_unreconciled()?,
                counts: s.machine_order_counts()?,
                orders,
                more,
            })
        })
        .await?)
}

fn page(
    state: &Context,
    session: Option<&Session>,
    show: Show,
    read: &Read,
    now: Timestamp,
) -> Orders {
    let count = |wanted: Option<OrderState>| -> u64 {
        read.counts
            .iter()
            .filter(|(state, _)| wanted.is_none_or(|wanted| *state == wanted))
            .map(|&(_, count)| count)
            .sum()
    };
    let chips = [
        (Show::All, tr("admin-all")),
        (Show::Unknown, tr("admin-unknown")),
        (Show::Cancelled, tr("admin-cancelled")),
    ]
    .into_iter()
    .map(|(filter, label)| Chip {
        href: filter.href(),
        label,
        count: count(filter.state()),
        current: filter == show,
    })
    .collect();
    let next = read
        .orders
        .last()
        .filter(|_| read.more)
        .map(|(last, _)| next_href(show.href(), last.id));
    let empty = read
        .orders
        .is_empty()
        .then(|| empty(show, count(show.state())));
    Orders {
        frame: Frame::new(
            state,
            Tab::MachineOrders,
            Tab::MachineOrders.label(),
            None,
            session,
            now,
        ),
        chips,
        paused: read.paused,
        rows: read
            .orders
            .iter()
            .map(|(order, cached)| row(state, order, cached.as_ref(), now))
            .collect(),
        empty,
        next,
    }
}

/// What a filter with nothing on this page says.
fn empty(show: Show, shown: u64) -> Empty {
    if shown > 0 {
        return Empty {
            good: false,
            title: tr("admin-no-more-orders"),
            text: tr("admin-this-page-is-past-the-last-order"),
            link: Some((Show::All.href(), format::first_page().into())),
        };
    }
    match show {
        Show::All => Empty {
            good: false,
            title: tr("admin-no-orders-yet"),
            text: tr("admin-orders-analyte-sends-from-a-lab-app-to-a"),
            link: None,
        },
        Show::Unknown => Empty {
            good: false,
            title: tr("admin-no-order-with-an-unknown-outcome"),
            text: tr("admin-every-order-sent-to-a-machine-has-a-known"),
            link: Some((Show::All.href(), tr("admin-show-all-orders").into())),
        },
        Show::Cancelled => Empty {
            good: false,
            title: tr("admin-no-cancelled-order"),
            text: tr("admin-no-order-was-cancelled-by-the-lab-app-and"),
            link: Some((Show::All.href(), tr("admin-show-all-orders").into())),
        },
    }
}

/// One order's row: Analyte's own number, its lab app and the version sent
/// stay on hover; test codes never leave the store.
fn row(
    state: &Context,
    order: &MachineOrder,
    cached: Option<&CachedOrder>,
    now: Timestamp,
) -> OrderRow {
    let first = order.items.first();
    let machine_cancelled = order
        .items
        .iter()
        .any(analyte_store::MachineOrderItem::analyzer_cancelled);
    let link = if machine_cancelled {
        Some((
            Tab::CancelledByMachine.href().to_owned(),
            tr("admin-review"),
        ))
    } else {
        first.map(|item| {
            (
                format::order_item_href(order.id, item.index),
                tr("admin-details"),
            )
        })
    };
    let mut status = look(order.state, &order.source_code);
    // The order's own state can read "Accepted" while the machine later
    // cancelled part of it; say so beside the Review link.
    if machine_cancelled && status.note.is_none() {
        status.note = Some(tr("admin-the-machine-cancelled-part-of-this-order").to_owned());
    }
    OrderRow {
        when: format::moment_unix(order.updated_at, now),
        order: order.key.order.as_str().to_owned(),
        order_exact: tr_args(
            "admin-lab-app-order-version-sent-machine-order",
            [
                (
                    "destination",
                    (order.key.destination.as_str()).to_string().into(),
                ),
                ("version", (order.version).to_string().into()),
                ("id", (order.id).to_string().into()),
            ],
        ),
        sample: cached.map(|cached| cached.sample.as_str().to_owned()),
        machine: order.key.machine.as_str().to_owned(),
        model: state
            .machine_model(order.key.machine.as_str())
            .map(str::to_owned),
        tests: tests(order),
        status,
        link,
    }
}

/// How many tests an order sent, counted over its items; none when an older
/// version did not record which tests an item covered. Never the tests.
pub(super) fn tests(order: &MachineOrder) -> Option<usize> {
    if order.items.is_empty() {
        return None;
    }
    order.items.iter().try_fold(0_usize, |sum, item| {
        item.coverage_known
            .then(|| sum.saturating_add(item.tests.len()))
    })
}

/// An order or item state as a status, as every page shows it
/// ([`format::order_look`]): the stored state on hover and, when the outcome
/// is unknown or the machine rejected it, what was recorded.
pub(super) fn look(state: OrderState, recorded: &analyte_model::SourceText) -> Look {
    let look = format::order_look(state);
    Look {
        tone: look.tone,
        icon: look.icon,
        word: look.word,
        exact: state.to_string(),
        note: matches!(state, OrderState::Uncertain | OrderState::Rejected).then(|| {
            tr_args(
                "admin-order-recorded",
                [("recorded", recorded.as_str().into())],
            )
        }),
    }
}
