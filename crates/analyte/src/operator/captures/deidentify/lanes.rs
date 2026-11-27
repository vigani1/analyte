//! Joins each direction's event bytes into one byte lane between link events, and splits them back.
//!
//! Every replacement keeps its length, so event boundaries, split and merged
//! reads and times survive exactly: a lane is cut back at the offsets it was
//! joined at.
use analyte_transport::recording::{Direction, Event};
use std::ops::Range;

/// One direction's bytes from one link session, joined in recorded order.
pub(super) struct Lane {
    /// Who sent these bytes.
    pub(super) direction: Direction,
    /// The joined bytes; replacements are written here.
    pub(super) bytes: Vec<u8>,
    /// The events joined into this lane: event index and where its bytes start.
    starts: Vec<(usize, usize)>,
}

impl Lane {
    /// The event that holds lane byte `index`, for error messages.
    pub(super) fn event_at(&self, index: usize) -> usize {
        let after = self.starts.partition_point(|(_, start)| *start <= index);
        self.starts
            .get(after.saturating_sub(1))
            .map_or(0, |(event, _)| *event)
    }
}

/// Where one event's bytes sit in its lane.
#[derive(Clone)]
pub(super) struct Place {
    /// The lane.
    pub(super) lane: usize,
    /// Its bytes in the lane.
    pub(super) bytes: Range<usize>,
}

/// Every lane of one recording, and where each event went.
pub(super) struct Lanes {
    /// The lanes, in the order their first event was recorded.
    pub(super) lanes: Vec<Lane>,
    /// For every event, its lane bytes; link events have none.
    pub(super) places: Vec<Option<Place>>,
}

impl Lanes {
    /// Joins the events; a link event (connect, disconnect, break) starts new lanes.
    pub(super) fn join(events: &[Event]) -> Self {
        let mut lanes: Vec<Lane> = Vec::new();
        let mut current: [Option<usize>; 2] = [None, None];
        let mut places = Vec::with_capacity(events.len());
        for (index, event) in events.iter().enumerate() {
            if event.link.is_some() {
                current = [None, None];
                places.push(None);
                continue;
            }
            let side = usize::from(event.direction == Direction::HostToMachine);
            let lane = match current.get(side).copied().flatten() {
                Some(lane) => lane,
                None => {
                    lanes.push(Lane {
                        direction: event.direction,
                        bytes: Vec::new(),
                        starts: Vec::new(),
                    });
                    let lane = lanes.len().saturating_sub(1);
                    if let Some(slot) = current.get_mut(side) {
                        *slot = Some(lane);
                    }
                    lane
                }
            };
            let Some(target) = lanes.get_mut(lane) else {
                places.push(None);
                continue;
            };
            let start = target.bytes.len();
            target.starts.push((index, start));
            target.bytes.extend_from_slice(&event.bytes);
            places.push(Some(Place {
                lane,
                bytes: start..target.bytes.len(),
            }));
        }
        Self { lanes, places }
    }

    /// The events again, with each event's bytes cut from its (rewritten) lane.
    pub(super) fn split(&self, events: &[Event]) -> Vec<Event> {
        events
            .iter()
            .zip(&self.places)
            .map(|(event, place)| {
                let bytes = place
                    .as_ref()
                    .and_then(|place| {
                        self.lanes
                            .get(place.lane)
                            .and_then(|lane| lane.bytes.get(place.bytes.clone()))
                    })
                    .map_or_else(|| event.bytes.clone(), <[u8]>::to_vec);
                Event {
                    direction: event.direction,
                    time_ms: event.time_ms,
                    bytes,
                    link: event.link,
                }
            })
            .collect()
    }
}
