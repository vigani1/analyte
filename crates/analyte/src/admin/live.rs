//! Serves GET /events: a server-sent event stream that tells a signed-in page which areas changed (Overview, Needs review, the sidebar's Needs review count, Machines, Deliveries, Activity), never what changed, with a heartbeat, a bounded lifetime and a cap on open streams.
//!
//! Where changes come from: every `check` (2 s) the stream compares the
//! store's marks (`analyte_store::ActivityMarks`: the newest row of each table
//! the live pages show, one index lookup each) and every machine's link light,
//! which the runtimes already publish on a watch channel. That is the
//! simplest correct source: it sees writes made by any part of Analyte and by
//! the command line, adds no hook to the store's write paths and holds no lock
//! across an await. A change stored between a page's render and its stream's
//! first read is not sent; the script reads the page again after every drop.
//!
//! Cancellation: the stream only reads; dropping it (the browser leaves, the
//! connection's lifetime ends, shutdown) leaves at most one queued store read,
//! whose answer is dropped.
use super::listener::Lifetime;
use super::server::Context;
use super::sessions::Session;
use super::view::{Tab, format, refusal::Refusal};
use crate::machine::link::status::{Light, Status};
use analyte_store::ActivityMarks;
use axum::{
    Extension,
    extract::{ConnectInfo, State},
    http::{HeaderValue, header},
    response::{
        IntoResponse as _, Response,
        sse::{Event, Sse},
    },
};
use futures_core::Stream;
use jiff::Timestamp;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context as Poll, Poll as Ready},
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    time::Instant,
};

/// How long one stream lives before it says goodbye and the browser opens a
/// new one: long enough that reconnects are rare, short enough that a stream
/// never outlives a session's check by much and every connection's end is
/// bounded.
pub(super) const LIFETIME: Duration = Duration::from_secs(300);

/// How often a stream says it is still there when nothing changed: the
/// sidebar's "Live" time moves at least this often, and a browser that went
/// away is noticed at the next write.
pub(super) const HEARTBEAT: Duration = Duration::from_secs(10);

/// How often a stream compares the store's marks and the link lights: the
/// longest a change waits before the page hears of it.
pub(super) const CHECK: Duration = Duration::from_secs(2);

/// How many streams may be open at once. The admin server has 32 connection
/// slots; four streams (a few tabs on the lab PC) leave 28 for pages.
pub(super) const STREAMS: usize = 4;

/// A stream's bounds; tests shorten them.
#[derive(Clone)]
pub(super) struct Bounds {
    pub(super) lifetime: Duration,
    pub(super) heartbeat: Duration,
    pub(super) check: Duration,
    /// One permit per open stream.
    pub(super) streams: Arc<Semaphore>,
}

impl Bounds {
    pub(super) fn standard() -> Self {
        Self {
            lifetime: LIFETIME,
            heartbeat: HEARTBEAT,
            check: CHECK,
            streams: Arc::new(Semaphore::new(STREAMS)),
        }
    }
}

/// An area of the pages that can change; a page names the areas it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    Overview,
    Review,
    /// The open Needs review count in every signed-in page's sidebar; named
    /// only when the count changed, so a page reloads only when it would
    /// show a different number.
    Count,
    Machines,
    Deliveries,
    Activity,
}

impl Area {
    const fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Review => "review",
            Self::Count => "count",
            Self::Machines => "machines",
            Self::Deliveries => "deliveries",
            Self::Activity => "activity",
        }
    }
}

/// The live area page `tab` shows, as its body names it; empty for a page
/// where only the sidebar's Needs review count ([`Area::Count`], which every
/// signed-in page shows) is live.
pub(super) const fn area(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => Area::Overview.name(),
        Tab::Activity => Area::Activity.name(),
        Tab::NeedsReview => Area::Review.name(),
        Tab::Machines => Area::Machines.name(),
        Tab::Deliveries => Area::Deliveries.name(),
        Tab::LabApps
        | Tab::Setup
        | Tab::MachineOrders
        | Tab::CancelledByMachine
        | Tab::DataKeeping
        | Tab::Settings => "",
    }
}

/// What a stream compares between checks.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    marks: Option<ActivityMarks>,
    lights: Vec<(Light, Timestamp, Option<Timestamp>)>,
}

impl Seen {
    /// The areas that differ between `self` and `now`; an unread store
    /// changes nothing.
    fn changed(&self, now: &Self) -> Vec<Area> {
        let mut areas = Vec::new();
        let mut add = |list: &[Area]| {
            for area in list {
                if !areas.contains(area) {
                    areas.push(*area);
                }
            }
        };
        if let (Some(was), Some(is)) = (self.marks, now.marks) {
            if (was.receipts, was.readings) != (is.receipts, is.readings) {
                add(&[Area::Overview, Area::Machines, Area::Activity]);
            }
            if (was.parked, was.open) != (is.parked, is.open) {
                add(&[Area::Review, Area::Overview, Area::Activity]);
            }
            if was.open != is.open {
                add(&[Area::Count]);
            }
            if (was.deliveries, was.attempts) != (is.deliveries, is.attempts) {
                add(&[Area::Deliveries, Area::Overview, Area::Activity]);
            }
            if (was.orders, was.audit) != (is.orders, is.audit) {
                add(&[Area::Overview, Area::Activity]);
            }
        }
        if self.lights != now.lights {
            add(&[Area::Machines, Area::Overview]);
        }
        areas
    }
}

/// One stream's state between its events.
struct Watch {
    state: Arc<Context>,
    /// The browser session it serves; none for the admin key, which no
    /// sign-out ends.
    session: Option<Arc<Session>>,
    lights: Vec<watch::Receiver<Status>>,
    seen: Option<Seen>,
    ends: Instant,
    beat: Instant,
    closing: watch::Receiver<bool>,
    said_bye: bool,
    _slot: OwnedSemaphorePermit,
}

impl Watch {
    /// The store's marks, read within one check, and the link lights now; an
    /// unread store gives no marks, and the next check tries again.
    async fn look(&self) -> Seen {
        let marks = match std::time::Instant::now().checked_add(self.state.live.check) {
            Some(deadline) => self
                .state
                .store
                .read::<_, analyte_store::OperatorError>(
                    deadline,
                    analyte_store::Store::activity_marks,
                )
                .await
                .inspect_err(|error| {
                    tracing::debug!(%error, "live stream could not read the store's marks");
                })
                .ok(),
            None => None,
        };
        Seen {
            marks,
            lights: self
                .lights
                .iter()
                .map(|light| {
                    let status = light.borrow();
                    (status.light(), status.changed_at(), status.last_data())
                })
                .collect(),
        }
    }

    /// Whether the browser session is still signed in; the admin key always is.
    fn signed_in(&self) -> bool {
        self.session.as_ref().is_none_or(|session| {
            matches!(
                self.state.sessions.lookup(Some(session.id.form_value())),
                Ok(Some(found)) if found.authenticated
            )
        })
    }

    /// The next event, or none when the stream ends: after its goodbye, at
    /// shutdown, or when its session was signed out.
    async fn next(mut self) -> Option<(Event, Self)> {
        if self.said_bye {
            return None;
        }
        if self.seen.is_none() {
            self.seen = Some(self.look().await);
            self.beat = after(self.state.live.heartbeat);
            return Some((live(&[]), self));
        }
        loop {
            let tick = tokio::time::sleep(self.state.live.check);
            let ends = self.ends;
            let ended = tokio::select! {
                () = tokio::time::sleep_until(ends) => true,
                _ = self.closing.wait_for(|&closing| closing) => return None,
                () = tick => false,
            };
            if ended {
                self.said_bye = true;
                return Some((Event::default().event("bye").data("lifetime"), self));
            }
            if !self.signed_in() {
                return None;
            }
            let now = self.look().await;
            let changed = self
                .seen
                .as_ref()
                .map_or_else(Vec::new, |seen| seen.changed(&now));
            if now.marks.is_some() {
                self.seen = Some(now);
            }
            if !changed.is_empty() || Instant::now() >= self.beat {
                self.beat = after(self.state.live.heartbeat);
                return Some((live(&changed), self));
            }
        }
    }
}

/// A `live` event: the PC's time to the second and the areas that changed.
fn live(changed: &[Area]) -> Event {
    let names: Vec<&str> = changed.iter().map(|area| area.name()).collect();
    Event::default().event("live").data(
        serde_json::json!({ "at": format::live_clock(Timestamp::now()), "changed": names })
            .to_string(),
    )
}

/// `duration` from now; at the clock's end, now, so a bound never grows.
fn after(duration: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(duration).unwrap_or(now)
}

/// One step of a stream: its next event and state, or its end.
type Step = Pin<Box<dyn Future<Output = Option<(Event, Watch)>> + Send>>;

/// The stream as the response body polls it: one pending step at a time.
struct Events(Option<Step>);

impl Stream for Events {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Poll<'_>) -> Ready<Option<Self::Item>> {
        let Some(step) = self.0.as_mut() else {
            return Ready::Ready(None);
        };
        match step.as_mut().poll(cx) {
            Ready::Pending => Ready::Pending,
            Ready::Ready(Some((event, watch))) => {
                self.0 = Some(Box::pin(watch.next()));
                Ready::Ready(Some(Ok(event)))
            }
            Ready::Ready(None) => {
                self.0 = None;
                Ready::Ready(None)
            }
        }
    }
}

/// GET /events. The router lets only a signed-in browser or the admin key
/// here. A stream holds one of [`STREAMS`] permits and keeps its connection
/// open for its lifetime and no longer; past the cap it is refused, and pages
/// still answer.
pub(super) async fn events(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    lifetime: Option<Extension<ConnectInfo<Lifetime>>>,
) -> Response {
    let Ok(slot) = state.live.streams.clone().try_acquire_owned() else {
        return Refusal::TooManyStreams.page(&state.assets);
    };
    let bounds = state.live.clone();
    if let Some(Extension(ConnectInfo(lifetime))) = lifetime {
        lifetime.keep_open_for(bounds.lifetime.saturating_add(super::server::ANSWER_TIME));
    }
    let watch = Watch {
        lights: state.machines.iter().map(|m| m.status.clone()).collect(),
        session: session.map(|Extension(session)| session),
        seen: None,
        ends: after(bounds.lifetime),
        beat: after(bounds.heartbeat),
        closing: state.closing.clone(),
        said_bye: false,
        state,
        _slot: slot,
    };
    let mut response = Sse::new(Events(Some(Box::pin(watch.next())))).into_response();
    // Not kept by any cache: each stream is this session's own.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(areas: &[Area]) -> Vec<&'static str> {
        areas.iter().map(|area| area.name()).collect()
    }

    // Owner decision 2026-11-20 (PR c2): each stored change names only the
    // areas that show it; a link light names Machines and Overview; a store
    // that could not be read names nothing, so no page reloads for it.
    #[test]
    fn each_change_names_the_areas_that_show_it() -> Result<(), jiff::Error> {
        let marks = ActivityMarks::default();
        let seen = Seen {
            marks: Some(marks),
            lights: vec![(Light::Off, Timestamp::UNIX_EPOCH, None)],
        };
        let with = |change: fn(&mut ActivityMarks)| {
            let mut now = marks;
            change(&mut now);
            Seen {
                marks: Some(now),
                lights: seen.lights.clone(),
            }
        };
        assert!(seen.changed(&seen).is_empty());
        assert_eq!(
            names(&seen.changed(&with(|m| m.receipts += 1))),
            ["overview", "machines", "activity"]
        );
        assert_eq!(
            names(&seen.changed(&with(|m| m.open -= 1))),
            ["review", "overview", "activity", "count"]
        );
        // A new item resolved within the same check leaves the count as it
        // was, so pages that show only the count do not reload.
        assert_eq!(
            names(&seen.changed(&with(|m| m.parked += 1))),
            ["review", "overview", "activity"]
        );
        assert_eq!(
            names(&seen.changed(&with(|m| m.attempts += 1))),
            ["deliveries", "overview", "activity"]
        );
        assert_eq!(
            names(&seen.changed(&with(|m| m.audit += 1))),
            ["overview", "activity"]
        );
        let light = Seen {
            marks: Some(marks),
            lights: vec![(Light::Live, Timestamp::from_second(1)?, None)],
        };
        assert_eq!(names(&seen.changed(&light)), ["machines", "overview"]);
        let unread = Seen {
            marks: None,
            lights: seen.lights.clone(),
        };
        assert!(seen.changed(&unread).is_empty());
        assert_eq!(area(Tab::Activity), "activity");
        assert_eq!(area(Tab::Setup), "");
        Ok(())
    }
}
