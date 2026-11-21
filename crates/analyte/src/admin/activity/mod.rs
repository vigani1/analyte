//! Serves Activity (GET /activity): what Analyte stored, newest first, with the Activity mockup's machine and type filters, today's counts and bounded pages.
//!
//! Every line is a row the store already kept (a message, a parked or resolved
//! item, a delivery try, an order's new state, an audited change), with the
//! time stored beside it. Machine links and disconnects are not stored, so
//! they are not listed, and the mockup's Connections filter is left out. No
//! script: the filters are one GET form, and each type is a submit button.
//!
//! Failed tries in a row of one delivery are one line, which links to the
//! same page listing every try of that delivery (`?delivery=`), one line each.
use crate::admin::error::CommandError;
use crate::admin::server::sessions::Session;
use crate::admin::view::{
    Tab, format,
    frame::Frame,
    list::{Empty, PAGE, one_page},
};
use crate::admin::{Error, server::Context};
use crate::deadline;
use analyte_model::DeliveryId;
use analyte_store::{Activity, ActivityCursor, ActivityGroup, ActivityLine};
use askama::Template as _;
use axum::{
    Extension,
    extract::{Query, State},
    response::Response,
};
use jiff::Timestamp;
use serde::Deserialize;
use std::sync::Arc;
mod page;
use page::{Day, Feed, FeedLine, MachineOption, TypeChip};

/// Which lines the page shows; the sidebar's link shows them all.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Show {
    #[default]
    All,
    Results,
    Orders,
    Problems,
    Changes,
}

impl Show {
    /// The type filters, in the order the page lists them.
    const ALL: [Self; 5] = [
        Self::All,
        Self::Results,
        Self::Orders,
        Self::Problems,
        Self::Changes,
    ];

    const fn group(self) -> Option<ActivityGroup> {
        match self {
            Self::All => None,
            Self::Results => Some(ActivityGroup::Results),
            Self::Orders => Some(ActivityGroup::Orders),
            Self::Problems => Some(ActivityGroup::Problems),
            Self::Changes => Some(ActivityGroup::Changes),
        }
    }

    /// Its value in the address, as the form sends it.
    const fn value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Results => "results",
            Self::Orders => "orders",
            Self::Problems => "problems",
            Self::Changes => "changes",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Results => "Results",
            Self::Orders => "Orders",
            Self::Problems => "Problems",
            Self::Changes => "Changes",
        }
    }
}

/// GET /activity's query: the machine (empty for all), the type and where
/// the page starts, written `at.source.key` as the last line's position; or
/// one delivery, whose tries are listed one by one instead.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct ActivityQuery {
    #[serde(default)]
    machine: String,
    #[serde(default)]
    show: Show,
    #[serde(default, deserialize_with = "cursor")]
    before: Option<ActivityCursor>,
    #[serde(default)]
    delivery: Option<DeliveryId>,
}

/// Which lines a page lists.
#[derive(Clone, Copy)]
enum Lines<'a> {
    /// The feed, for `machine` (none for all) and `show`.
    Feed {
        machine: Option<&'a str>,
        show: Show,
    },
    /// Every try of one delivery.
    Tries(DeliveryId),
}

/// Reads `at.source.key`; anything else is refused, as an unreadable address.
fn cursor<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ActivityCursor>, D::Error> {
    let written = String::deserialize(deserializer)?;
    let mut parts = written.splitn(3, '.').map(str::parse::<i64>);
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(at)), Some(Ok(source)), Some(Ok(key))) => {
            Ok(Some(ActivityCursor { at, source, key }))
        }
        _ => Err(serde::de::Error::custom(
            "expected the position of a line, as at.source.key",
        )),
    }
}

/// What the page reads in one store request.
struct Read {
    lines: Vec<ActivityLine>,
    more: bool,
    /// Today's lines per type; none when the PC's day could not be worked out.
    today: Option<Vec<(ActivityGroup, u64)>>,
}

pub(in crate::admin) async fn activity(
    State(state): State<Arc<Context>>,
    Query(query): Query<ActivityQuery>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let machine = (!query.machine.is_empty()).then_some(query.machine);
    let (show, before, delivery) = (query.show, query.before, query.delivery);
    let lines = |machine| match delivery {
        Some(delivery) => Lines::Tries(delivery),
        None => Lines::Feed { machine, show },
    };
    crate::admin::more::show(
        &state,
        session.as_ref(),
        Tab::Activity,
        read(&state, machine.clone(), show, before, delivery),
        |read, session, now| {
            feed(
                &state,
                session,
                lines(machine.as_deref()),
                before,
                &read,
                now,
            )
            .render()
        },
    )
    .await
}

async fn read(
    state: &Context,
    machine: Option<String>,
    show: Show,
    before: Option<ActivityCursor>,
    delivery: Option<DeliveryId>,
) -> Result<Read, Error> {
    let day = format::today_span(Timestamp::now());
    let group = show.group();
    Ok(state
        .store
        .read::<_, CommandError>(deadline()?, move |s| {
            let machine = machine.as_deref();
            let start = before.unwrap_or(ActivityCursor::NEWEST);
            let page = |before, limit| match delivery {
                Some(delivery) => s.delivery_activity(delivery, before, limit),
                None => s.activity(machine, group, before, limit),
            };
            let (lines, more) =
                one_page::<_, CommandError>(page(start, PAGE + 1)?, PAGE, |last| {
                    Ok(!page(last.cursor, 1)?.is_empty())
                })?;
            // One delivery's tries have no filters to count for.
            let today = day
                .filter(|_| delivery.is_none())
                .map(|day| s.activity_counts(machine, day))
                .transpose()?;
            Ok(Read { lines, more, today })
        })
        .await?)
}

fn feed(
    state: &Context,
    session: Option<&Session>,
    lines: Lines<'_>,
    before: Option<ActivityCursor>,
    read: &Read,
    now: Timestamp,
) -> Feed {
    let (machine, show) = match lines {
        Lines::Feed { machine, show } => (machine, show),
        Lines::Tries(_) => (None, Show::All),
    };
    let count = |group: ActivityGroup| {
        read.today.as_ref().map(|today| {
            today
                .iter()
                .find(|(counted, _)| *counted == group)
                .map_or(0, |&(_, count)| count)
        })
    };
    let today = read.today.as_ref().map(|today| {
        let count: u64 = today.iter().map(|&(_, count)| count).sum();
        (count, format::events(count))
    });
    let mut machines: Vec<MachineOption> = state
        .machines
        .iter()
        .map(|m| MachineOption {
            id: m.id.as_str().to_owned(),
            label: label(state, m.id.as_str()),
            selected: machine == Some(m.id.as_str()),
        })
        .collect();
    // A machine no longer set up keeps its history; the form still names it.
    if let Some(chosen) = machine.filter(|id| !machines.iter().any(|m| m.id == *id)) {
        machines.push(MachineOption {
            id: chosen.to_owned(),
            label: chosen.to_owned(),
            selected: true,
        });
    }
    let earlier = read
        .lines
        .last()
        .filter(|_| read.more)
        .map(|last| href(lines, last.cursor));
    let empty = read.lines.is_empty().then(|| {
        if before.is_some() {
            Empty {
                good: false,
                title: "No earlier activity",
                text: "This page is past the oldest line Analyte keeps.",
                link: Some((Tab::Activity.href(), "Back to the newest".into())),
            }
        } else if matches!(lines, Lines::Tries(_)) {
            Empty {
                good: false,
                title: "No tries kept",
                text: "Analyte keeps no try of this delivery.",
                link: Some((Tab::Activity.href(), "Show all activity".into())),
            }
        } else if machine.is_some() || show != Show::All {
            Empty {
                good: false,
                title: "Nothing for this filter",
                text: "No stored activity matches this machine and type.",
                link: Some((Tab::Activity.href(), "Show all activity".into())),
            }
        } else {
            Empty {
                good: false,
                title: "No activity yet",
                text: concat!(
                    "Messages from machines, deliveries to lab apps, orders and changes ",
                    "are listed here once Analyte stores them.",
                ),
                link: None,
            }
        }
    });
    Feed {
        frame: Frame::new(
            state,
            Tab::Activity,
            Tab::Activity.label(),
            None,
            session,
            now,
        ),
        machines,
        one_delivery: matches!(lines, Lines::Tries(_)),
        show: show.value(),
        chips: Show::ALL
            .into_iter()
            .map(|filter| TypeChip {
                value: filter.value(),
                label: filter.label(),
                count: filter.group().and_then(count),
                pressed: filter == show,
            })
            .collect(),
        today,
        days: days(state, &read.lines, now),
        empty,
        earlier,
    }
}

/// The lines grouped by the day they fall in, in order; lines without a
/// stored time come last, under their own divider.
fn days(state: &Context, lines: &[ActivityLine], now: Timestamp) -> Vec<Day> {
    let mut days: Vec<Day> = Vec::new();
    for line in lines {
        let day = line
            .at
            .and_then(|at| Timestamp::from_second(at).ok())
            .map_or_else(
                || format::FEED_UNDATED.to_owned(),
                |at| format::feed_day(at, now),
            );
        let said = format::said(
            &line.what,
            line.at,
            line.machine.as_deref().map(|id| format::Named {
                id,
                model: model(state, id),
            }),
            crate::admin::parked::headline,
        );
        let shown = FeedLine {
            key: key(line),
            at: line.at.map(format::feed_time),
            tone: said.tone,
            icon: said.icon,
            word: said.word,
            head: said.head,
            rest: said.rest,
            link: said.link,
        };
        match days.last_mut() {
            Some(last) if last.label == day => last.lines.push(shown),
            Some(_) | None => days.push(Day {
                label: day,
                lines: vec![shown],
            }),
        }
    }
    days
}

/// A line's key for the live script: its position, or for failed tries in a
/// row their delivery and first try, so a new try updates the line instead
/// of counting as a new one.
fn key(line: &ActivityLine) -> String {
    match &line.what {
        Activity::Attempt {
            delivery,
            tries: Some(tries),
            ..
        } => format!("tries.{}.{}", uuid::Uuid::from(*delivery), tries.first),
        Activity::Attempt { tries: None, .. }
        | Activity::Stored(_)
        | Activity::Parked { .. }
        | Activity::Resolved { .. }
        | Activity::Order { .. }
        | Activity::Changed { .. } => format!(
            "{}.{}.{}",
            line.cursor.at, line.cursor.source, line.cursor.key
        ),
    }
}

/// The model of machine `id` when it is set up and its definition names one.
fn model<'a>(state: &'a Context, id: &str) -> Option<&'a str> {
    state
        .machines
        .iter()
        .find(|m| m.id.as_str() == id)
        .and_then(|m| m.model.as_deref())
}

/// A machine as the filter names it: its ID, with its model beside it.
fn label(state: &Context, id: &str) -> String {
    model(state, id).map_or_else(|| id.to_owned(), |model| format!("{id} ({model})"))
}

/// The page of `lines` that starts after `before`.
fn href(lines: Lines<'_>, before: ActivityCursor) -> String {
    let mut parts = Vec::new();
    match lines {
        Lines::Feed { machine, show } => {
            if let Some(machine) = machine {
                parts.push(format!("machine={}", format::path_segment(machine)));
            }
            if show != Show::All {
                parts.push(format!("show={}", show.value()));
            }
        }
        Lines::Tries(delivery) => {
            parts.push(format!("delivery={}", uuid::Uuid::from(delivery)));
        }
    }
    parts.push(format!(
        "before={}.{}.{}",
        before.at, before.source, before.key
    ));
    format!("{}?{}", Tab::Activity.href(), parts.join("&"))
}
