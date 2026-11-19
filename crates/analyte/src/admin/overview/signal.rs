//! Builds the signal path from observed state: one node per set-up machine and per lab app, and the figures below it.
//!
//! Machines come from their runtime link light and stored receipts; lab apps
//! from their store health, the last accepted delivery and, for a kept lab app
//! whose keys were missing at startup, the key source. Nothing is guessed: a
//! figure without data behind it is left out.
use super::Activity;
use super::attention::Facts;
use super::page::{Figure, LabNode, MachineNode, Wire};
use crate::admin::Error;
use crate::admin::removed::Kept;
use crate::admin::server::{Connection, Context};
use crate::admin::view::{Tone, format, icon::Icon};
use crate::machine::link::status::Light;
use analyte_model::Identifier;
use analyte_store::{Health, LastDelivered};
use jiff::Timestamp;
use std::{collections::BTreeMap, sync::atomic::Ordering};

/// One node per set-up machine, in setup order. Each machine whose service
/// stopped or whose link is off is added to `facts` with what was observed.
pub(super) fn machines(
    state: &Context,
    activity: &BTreeMap<String, Activity>,
    facts: &mut Facts,
    now: Timestamp,
) -> Vec<MachineNode> {
    state
        .machines
        .iter()
        .zip(0usize..)
        .map(|(machine, index)| {
            let status = machine.status.borrow().clone();
            let light = status.light();
            let running = machine.running.load(Ordering::Acquire);
            let id = machine.id.as_str();
            let sentence = format::link_sentence(
                &machine.connection,
                light,
                &format::clock(status.changed_at(), now),
                id,
            );
            if !running {
                facts.stopped.push(machine.id.clone());
            }
            if light == Light::Off {
                facts.unlinked.push((
                    machine.id.clone(),
                    status.state(),
                    format!("{sentence}. {}", format::link_advice(&machine.connection)),
                ));
            }
            let (tone, state, wire) = match (running, light) {
                (false, _) => (Tone::Problem, "Stopped in Analyte", Wire::Cut),
                (true, Light::Live) => (Tone::Good, status.state(), Wire::Live),
                (true, Light::Waiting) => (Tone::Attention, status.state(), Wire::Wait),
                (true, Light::Off) => (Tone::Problem, status.state(), Wire::Cut),
            };
            let stored = activity.get(id).and_then(|seen| seen.last);
            MachineNode {
                id: id.to_owned(),
                model: machine.model.clone(),
                tone,
                state: state.to_owned(),
                last_data: status
                    .last_data()
                    .max(stored)
                    .map(|at| format::moment(at, now)),
                kind: format::connection_kind(&machine.connection),
                kind_icon: connection_icon(&machine.connection),
                wire,
                dot: match index % 3 {
                    0 => "",
                    1 => "wire__dot--2",
                    _ => "wire__dot--3",
                },
                line: match (running, light) {
                    (false, _) => Some("Analyte stopped working with this machine".into()),
                    (true, Light::Live) => None,
                    (true, Light::Waiting | Light::Off) => Some(sentence),
                },
            }
        })
        .collect()
}

const fn connection_icon(connection: &Connection) -> Icon {
    match connection {
        Connection::Serial(_) => Icon::Serial,
        Connection::UsbAdapter(_) => Icon::Usb,
        Connection::Listening(_) | Connection::Dialing => Icon::Network,
        Connection::Folder(_) => Icon::Folder,
    }
}

/// The lab app nodes and the figures under the path: first the lab apps
/// set-up machines send to, then those kept for removed machines. Paused
/// sending is added to `facts`.
pub(super) async fn lab_apps(
    state: &Context,
    delivered: &LastDelivered,
    facts: &mut Facts,
    now: Timestamp,
) -> Result<(Vec<LabNode>, Vec<Figure>), Error> {
    let mut nodes = Vec::new();
    let mut lists = Vec::new();
    let mut waiting = Waiting::default();
    for destination in &state.destinations {
        let health = destination.health(&state.store, now.as_second()).await?;
        if health.paused {
            facts
                .paused
                .push((destination.id().clone(), health.pending_count));
        }
        waiting.add(&health);
        let senders: Vec<&str> = state
            .machines
            .iter()
            .filter(|machine| &machine.lab_app == destination.id())
            .map(|machine| machine.id.as_str())
            .collect();
        nodes.push(lab_node(
            destination.id(),
            &health,
            delivered,
            &senders,
            None,
            now,
        ));
        lists.push(order_list(destination.id(), &health, now));
    }
    for (name, kept) in &state.retired {
        let id = Identifier::new(name)?;
        let read = id.clone();
        let second = now.as_second();
        let health = state
            .store
            .request(crate::deadline()?, move |store| {
                store.health(&read, second, 0)
            })
            .await?;
        waiting.add(&health);
        let keys = if kept.keys_missing {
            Some(missing_keys(state, kept).await?)
        } else {
            None
        };
        nodes.push(lab_node(&id, &health, delivered, &[], keys, now));
    }
    let mut figures = Vec::new();
    if !nodes.is_empty() {
        figures.extend(waiting.figures(now));
    }
    figures.extend(lists);
    Ok((nodes, figures))
}

fn lab_node(
    name: &Identifier,
    health: &Health,
    delivered: &LastDelivered,
    senders: &[&str],
    keys: Option<String>,
    now: Timestamp,
) -> LabNode {
    let last = delivered
        .at
        .iter()
        .find(|(destination, _)| destination == name)
        .and_then(|&(_, time)| Timestamp::from_second(time).ok());
    let mut lines = vec![
        format::last_delivered(last, delivered.purged, now),
        format::waiting(health.pending_count, oldest(health, now), now),
        format::senders(senders),
    ];
    let blocked = keys.is_some();
    lines.extend(keys);
    let (tone, status, wire) = if blocked {
        (Tone::Problem, "Cannot deliver", Wire::Cut)
    } else if health.paused {
        (Tone::Attention, "Sending paused", Wire::Wait)
    } else if health.pending_count > 0 {
        (Tone::Neutral, "Queued", Wire::Live)
    } else {
        (Tone::Good, "Up to date", Wire::Live)
    };
    LabNode {
        name: name.as_str().to_owned(),
        tone,
        status,
        wire,
        lines,
        todo: blocked.then_some(format::KEYS_ADVICE),
    }
}

/// When the oldest waiting item arrived, when the store knows its age.
fn oldest(health: &Health, now: Timestamp) -> Option<Timestamp> {
    health
        .oldest_pending_age
        .and_then(|age| i64::try_from(age).ok())
        .and_then(|age| now.as_second().checked_sub(age))
        .and_then(|second| Timestamp::from_second(second).ok())
}

/// Which of a kept lab app's keys the key source has no value for now; the
/// same rule the service loader and the Lab apps page use.
async fn missing_keys(state: &Context, kept: &Kept) -> Result<String, Error> {
    use crate::installation::{credentials::Locked, secrets};
    let directory = state.directory.clone();
    let names = [
        ("API key", kept.api_key.clone()),
        ("signing key", kept.signing_key.clone()),
    ];
    Ok(tokio::task::spawn_blocking(move || {
        let stored = match Locked::new(&directory).read() {
            Ok(stored) => stored,
            Err(error) => return format!("Its keys cannot be checked: {error}."),
        };
        let missing: Vec<&str> = names
            .iter()
            .filter(|(_, name)| {
                matches!(
                    secrets::source(name.as_str(), |key| std::env::var(key).ok(), &stored),
                    secrets::Source::Missing
                )
            })
            .map(|&(label, _)| label)
            .collect();
        format::missing_keys(&missing)
    })
    .await?)
}

/// How fresh one lab app's order list is: only from what its health records.
fn order_list(name: &Identifier, health: &Health, now: Timestamp) -> Figure {
    let checked = health
        .last_successful_poll
        .and_then(|second| Timestamp::from_second(second).ok());
    let (tone, value) = match (health.cache_trusted, checked) {
        (true, _) => (Tone::Good, "Up to date"),
        (false, Some(_)) => (Tone::Attention, "Out of date"),
        (false, None) => (Tone::Neutral, "Not checked yet"),
    };
    Figure {
        label: format!("Order list · {}", name.as_str()),
        value: value.into(),
        when: None,
        aside: if checked.is_some() {
            "checked".into()
        } else {
            String::new()
        },
        aside_when: checked.map(|at| format::moment(at, now)),
        tone: Some(tone),
    }
}

/// What waits for every lab app together.
#[derive(Default)]
struct Waiting {
    count: u64,
    /// The largest known age, in seconds.
    oldest: Option<u64>,
    /// Some item waits whose age the store does not know.
    unknown: bool,
}

impl Waiting {
    fn add(&mut self, health: &Health) {
        self.count = self.count.saturating_add(health.pending_count);
        if health.pending_count > 0 {
            self.unknown |= health.oldest_pending_age.is_none();
            self.oldest = self.oldest.max(health.oldest_pending_age);
        }
    }

    fn figures(&self, now: Timestamp) -> [Figure; 2] {
        let items = Figure {
            label: "Items waiting to send".into(),
            value: self.count.to_string(),
            when: None,
            aside: "results and order messages".into(),
            aside_when: None,
            tone: None,
        };
        let since = self
            .oldest
            .and_then(|age| i64::try_from(age).ok())
            .and_then(|age| now.as_second().checked_sub(age))
            .and_then(|second| Timestamp::from_second(second).ok());
        let oldest = match (self.count, self.unknown, since) {
            (0, _, _) => worded("Oldest waiting", Tone::Good, "Nothing waiting"),
            (_, false, Some(at)) => Figure {
                label: "Oldest waiting".into(),
                value: String::new(),
                when: Some(format::moment(at, now)),
                aside: format::ago(at, now),
                aside_when: None,
                tone: None,
            },
            _ => worded("Oldest waiting", Tone::Neutral, "Not known"),
        };
        [items, oldest]
    }
}

fn worded(label: &str, tone: Tone, value: &str) -> Figure {
    Figure {
        label: label.into(),
        value: value.into(),
        when: None,
        aside: String::new(),
        aside_when: None,
        tone: Some(tone),
    }
}
