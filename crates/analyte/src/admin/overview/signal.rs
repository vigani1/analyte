//! Builds the signal path from observed state: one node per set-up machine and per lab app, and the figures below it.
//!
//! Machines come from their runtime link light and stored receipts; lab apps
//! from their store health, the last accepted delivery, failed tries since it
//! and, for a kept lab app whose keys were missing at startup, the key source.
//! Nothing is guessed: a figure without data behind it is left out.
use super::attention::Facts;
use super::page::{Figure, LabNode, MachineNode, Wire};
use crate::admin::Error;
use crate::admin::removed::Kept;
use crate::admin::server::Context;
use crate::admin::view::{Tone, format, format::Line, icon::Icon};
use crate::machine::link::status::Light;
use analyte_model::Identifier;
use analyte_store::{FailingDestination, Health, LastDelivered};
use jiff::Timestamp;
use std::{collections::BTreeMap, sync::atomic::Ordering};

/// One node per set-up machine, in setup order. Each machine whose service
/// stopped or whose link is off is added to `facts` with what was observed.
pub(super) fn machines(
    state: &Context,
    activity: &BTreeMap<String, Timestamp>,
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
                (false, _) => (Tone::Problem, format::STOPPED, Wire::Cut),
                (true, Light::Live) => (Tone::Good, status.state(), Wire::Live),
                (true, Light::Waiting) => (Tone::Neutral, status.state(), Wire::Wait),
                (true, Light::Off) => (Tone::Problem, status.state(), Wire::Cut),
            };
            let stored = activity.get(id).copied();
            MachineNode {
                id: id.to_owned(),
                href: format::machine_href(id),
                model: machine.model.clone(),
                tone,
                state: state.to_owned(),
                last_data: status
                    .last_data()
                    .max(stored)
                    .map(|at| format::moment(at, now)),
                kind: format::connection_kind(&machine.connection),
                kind_icon: Icon::connection(&machine.connection),
                wire,
                dot: match index % 3 {
                    0 => "",
                    1 => "wire__dot--2",
                    _ => "wire__dot--3",
                },
                line: match (running, light) {
                    (false, _) => Some(format::STOPPED_SENTENCE.into()),
                    (true, Light::Live) => None,
                    (true, Light::Waiting | Light::Off) => Some(sentence),
                },
            }
        })
        .collect()
}

/// The lab app nodes and the figures under the path: first the lab apps
/// set-up machines send to, then those kept for removed machines. Paused
/// sending and failing tries are added to `facts`.
pub(super) async fn lab_apps(
    state: &Context,
    delivered: &LastDelivered,
    failing: &[FailingDestination],
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
        let (node, observed) = lab_node(
            destination.id(),
            &health,
            delivered,
            failing,
            &senders,
            None,
            now,
        );
        if let Some(observed) = observed {
            facts
                .failing
                .push((destination.id().clone(), health.pending_count, observed));
        }
        nodes.push(node);
        lists.push(order_list_figure(destination.id(), &health, now));
    }
    for (name, kept) in &state.retired {
        let id = Identifier::new(name)?;
        let (health, keys) = kept_health(state, &id, Some(kept), now).await?;
        waiting.add(&health);
        let (node, observed) = lab_node(&id, &health, delivered, failing, &[], keys, now);
        if let Some(observed) = observed {
            facts
                .failing
                .push((id.clone(), health.pending_count, observed));
        }
        nodes.push(node);
    }
    let mut figures = Vec::new();
    if !nodes.is_empty() {
        figures.extend(waiting.figures(now));
    }
    figures.extend(lists);
    Ok((nodes, figures))
}

/// How a lab app's deliveries stand, by the rules the Overview's lab node
/// and the Lab apps page share. Missing keys and paused sending come first:
/// no try is made then, so failing would mislead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Delivery {
    /// Its keys were missing at startup, so Analyte makes no try.
    CannotDeliver,
    /// A delivery paused sending.
    Paused,
    /// Items wait and every try since the last delivery failed.
    NotDelivering,
    /// Items wait and nothing stops them.
    Queued,
    /// Nothing waits.
    UpToDate,
}

impl Delivery {
    pub(in crate::admin) const fn tone(self) -> Tone {
        match self {
            Self::CannotDeliver | Self::NotDelivering => Tone::Problem,
            Self::Paused => Tone::Attention,
            Self::Queued => Tone::Neutral,
            Self::UpToDate => Tone::Good,
        }
    }

    pub(in crate::admin) const fn word(self) -> &'static str {
        match self {
            Self::CannotDeliver => "Cannot deliver",
            Self::Paused => "Sending paused",
            Self::NotDelivering => "Not delivering",
            Self::Queued => "Queued",
            Self::UpToDate => "Up to date",
        }
    }
}

/// How one lab app stands, with the lines that say why.
pub(in crate::admin) struct Standing {
    pub(in crate::admin) delivery: Delivery,
    /// Which keys were missing at startup and what the key source says now;
    /// only when it cannot deliver.
    pub(in crate::admin) keys: Option<Keys>,
    /// What was observed about tries that keep failing; only when not delivering.
    pub(in crate::admin) failing: Option<Line>,
    /// "Last delivered 10:43", "Nothing delivered yet" or "No delivery in the kept records".
    pub(in crate::admin) delivered: Line,
    /// "4 items waiting · oldest 10:31" or "nothing waiting".
    pub(in crate::admin) waiting: Line,
}

/// How lab app `name` stands from its store `health`, when each lab app last
/// accepted a delivery, which keep failing and, for a kept one whose keys were
/// missing at startup, its `keys`.
pub(in crate::admin) fn standing(
    name: &Identifier,
    health: &Health,
    delivered: &LastDelivered,
    failing: &[FailingDestination],
    keys: Option<Keys>,
    now: Timestamp,
) -> Standing {
    let last = delivered
        .at
        .iter()
        .find(|(destination, _)| destination == name)
        .and_then(|&(_, time)| Timestamp::from_second(time).ok());
    let failing = failing
        .iter()
        .find(|failing| &failing.destination == name)
        .filter(|_| keys.is_none() && !health.paused)
        .map(|failing| format::failing(failing, now));
    let delivery = if keys.is_some() {
        Delivery::CannotDeliver
    } else if health.paused {
        Delivery::Paused
    } else if failing.is_some() {
        Delivery::NotDelivering
    } else if health.pending_count > 0 {
        Delivery::Queued
    } else {
        Delivery::UpToDate
    };
    Standing {
        delivery,
        keys,
        failing,
        delivered: format::last_delivered(last, delivered.purged, now),
        waiting: format::waiting(health.pending_count, oldest(health, now), now),
    }
}

/// One lab app's node and, when its tries keep failing and nothing else stops
/// it, what was observed in plain words for its to-do line.
fn lab_node(
    name: &Identifier,
    health: &Health,
    delivered: &LastDelivered,
    failing: &[FailingDestination],
    senders: &[&str],
    keys: Option<Keys>,
    now: Timestamp,
) -> (LabNode, Option<String>) {
    let Standing {
        delivery,
        keys,
        failing,
        delivered,
        waiting,
    } = standing(name, health, delivered, failing, keys, now);
    let observed = failing.as_ref().map(Line::plain);
    let (keys, support) = keys.map_or((None, None), |keys| (Some(keys.line), keys.support));
    // Why it cannot deliver comes first, as the to-do line leads with it.
    let mut lines: Vec<Line> = keys.into_iter().map(Line::text).collect();
    lines.extend(failing);
    lines.push(delivered.then(waiting));
    lines.push(Line::text(format::senders(senders)));
    let blocked = delivery == Delivery::CannotDeliver;
    let node = LabNode {
        name: name.as_str().to_owned(),
        tone: delivery.tone(),
        status: delivery.word(),
        wire: match delivery {
            Delivery::CannotDeliver | Delivery::NotDelivering => Wire::Cut,
            Delivery::Paused => Wire::Wait,
            Delivery::Queued | Delivery::UpToDate => Wire::Live,
        },
        // The break mark says why the line is cut: a key for missing keys, a
        // cross for tries that fail; Analyte makes no try without keys.
        mark: if blocked { Icon::Key } else { Icon::Cross },
        lines,
        support: support.into_iter().collect(),
        todo: blocked.then_some(format::KEYS_ADVICE),
    };
    (node, observed)
}

/// What a kept lab app's node says about its keys: the line people read and,
/// when the keys could not be checked, the exact reason for support.
pub(in crate::admin) struct Keys {
    pub(in crate::admin) line: String,
    pub(in crate::admin) support: Option<String>,
}

/// When the oldest waiting item arrived, when the store knows its age.
fn oldest(health: &Health, now: Timestamp) -> Option<Timestamp> {
    health
        .oldest_pending_age
        .and_then(|age| i64::try_from(age).ok())
        .and_then(|age| now.as_second().checked_sub(age))
        .and_then(|second| Timestamp::from_second(second).ok())
}

/// What the store says about lab app `id` when no running machine uses it,
/// so no freshness applies, and, for a kept one whose keys were missing at
/// startup, which of them the key source lacks now.
pub(in crate::admin) async fn kept_health(
    state: &Context,
    id: &Identifier,
    kept: Option<&Kept>,
    now: Timestamp,
) -> Result<(Health, Option<Keys>), Error> {
    let read = id.clone();
    let second = now.as_second();
    let health = state
        .store
        .read(crate::deadline()?, move |store| {
            store.health(&read, second, 0)
        })
        .await?;
    let keys = match kept {
        Some(kept) if kept.keys_missing => Some(missing_keys(state, kept).await?),
        Some(_) | None => None,
    };
    Ok((health, keys))
}

/// Which of a kept lab app's keys the key source has no value for now; the
/// same rule the service loader and the Lab apps page use.
async fn missing_keys(state: &Context, kept: &Kept) -> Result<Keys, Error> {
    use crate::installation::{credentials::Locked, secrets};
    let directory = state.directory.clone();
    let names = [
        ("API key", kept.api_key.clone()),
        ("signing key", kept.signing_key.clone()),
    ];
    Ok(tokio::task::spawn_blocking(move || {
        let stored = match Locked::new(&directory).read() {
            Ok(stored) => stored,
            Err(error) => {
                return Keys {
                    line: "Its keys cannot be checked.".into(),
                    support: Some(error.to_string()),
                };
            }
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
        Keys {
            line: format::missing_keys(&missing),
            support: None,
        }
    })
    .await?)
}

/// How fresh a lab app's order list is, only from what its health records:
/// its tone, its words and when it was last checked.
pub(in crate::admin) fn order_list(health: &Health) -> (Tone, &'static str, Option<Timestamp>) {
    let checked = health
        .last_successful_poll
        .and_then(|second| Timestamp::from_second(second).ok());
    let (tone, value) = match (health.cache_trusted, checked) {
        (true, _) => (Tone::Good, "Up to date"),
        (false, Some(_)) => (Tone::Attention, "Out of date"),
        (false, None) => (Tone::Neutral, "Not checked yet"),
    };
    (tone, value, checked)
}

/// The figure for one lab app's order list.
fn order_list_figure(name: &Identifier, health: &Health, now: Timestamp) -> Figure {
    let (tone, value, checked) = order_list(health);
    Figure {
        label: format!("Order list · {}", name.as_str()),
        value: value.into(),
        when: None,
        aside: if checked.is_some() {
            "· checked".into()
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
            (0, _, _) => Figure {
                label: "Oldest waiting".into(),
                value: "—".into(),
                when: None,
                aside: "nothing waiting".into(),
                aside_when: None,
                tone: None,
            },
            (_, false, Some(at)) => Figure {
                label: "Oldest waiting".into(),
                value: String::new(),
                when: Some(format::moment(at, now)),
                aside: format::ago(at, now),
                aside_when: None,
                tone: None,
            },
            _ => worded("Oldest waiting", Tone::Neutral, format::NOT_RECORDED),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::view::format::Part;
    use askama::Template as _;

    /// One lab app node on its own, as the signal path draws it.
    #[derive(askama::Template)]
    #[template(
        source = "{% import \"components/display.html\" as display %}{% call display::lab_node(lab) %}{% endcall %}",
        ext = "html"
    )]
    struct LabShown {
        lab: LabNode,
    }

    /// One figure on its own, as the figures row draws it.
    #[derive(askama::Template)]
    #[template(
        source = "{% import \"components/display.html\" as display %}{% call display::figure(item) %}{% endcall %}",
        ext = "html"
    )]
    struct FigureShown {
        item: Figure,
    }

    fn now() -> Timestamp {
        Timestamp::from_second(1_795_046_077).expect("a valid test time")
    }

    fn health(pending: u64, age: Option<u64>, paused: bool) -> Health {
        Health {
            pending_count: pending,
            pending_bytes: 0,
            oldest_pending_age: age,
            paused,
            last_successful_poll: None,
            snapshot_complete: false,
            cache_trusted: false,
        }
    }

    fn lab(health: &Health, delivered: &LastDelivered, keys: Option<&str>) -> String {
        lab_failing(health, delivered, &[], keys).0
    }

    /// The rendered node and what its to-do line would say, with `failing` as the store read it.
    fn lab_failing(
        health: &Health,
        delivered: &LastDelivered,
        failing: &[FailingDestination],
        keys: Option<&str>,
    ) -> (String, Option<String>) {
        let name = Identifier::new("main-lab").expect("a valid test name");
        let (lab, observed) = lab_node(
            &name,
            health,
            delivered,
            failing,
            &["chemistry-1"],
            keys.map(|line| Keys {
                line: line.to_owned(),
                support: None,
            }),
            now(),
        );
        let html = LabShown { lab }.render().expect("the lab node renders");
        (html, observed)
    }

    fn figure(figure: Figure) -> String {
        FigureShown { item: figure }
            .render()
            .expect("the figure renders")
    }

    // Owner request 2026-11-19 (overview states): a lab app reads "Up to date"
    // with nothing waiting, "Queued" while items wait and it can send, "Sending
    // paused" when a delivery paused it, and "Cannot deliver" with what to do
    // when its keys are missing; each from the store's own health.
    #[test]
    fn each_lab_app_state_has_its_status_wire_and_lines() {
        let delivered = LastDelivered {
            at: vec![(
                Identifier::new("main-lab").expect("a valid test name"),
                now().as_second() - 120,
            )],
            purged: false,
        };
        let good = lab(&health(0, None, false), &delivered, None);
        assert!(
            good.contains("<span class=\"status status--good\">"),
            "{good}"
        );
        assert!(good.contains("Up to date</span>"));
        assert!(
            good.contains("<span class=\"wire \" aria-hidden=\"true\"><span class=\"wire__dot")
        );
        assert!(good.contains("<p class=\"node__line\">Last delivered <time "));
        assert!(good.contains("</time> · nothing waiting</p>"));
        assert!(good.contains("<p class=\"node__line\">From chemistry-1</p>"));

        let queued = lab(&health(3, Some(600), false), &delivered, None);
        assert!(
            queued.contains("<span class=\"status status--off\">"),
            "{queued}"
        );
        assert!(queued.contains("Queued</span>"));
        assert!(queued.contains(" · 3 items waiting · oldest <time "));
        assert!(!queued.contains("node--bad"));

        let paused = lab(&health(2, None, true), &delivered, None);
        assert!(
            paused.contains("<span class=\"status status--warn\">"),
            "{paused}"
        );
        assert!(paused.contains("Sending paused</span>"));
        assert!(paused.contains("<span class=\"wire wire--wait\""));
        assert!(paused.contains(" · 2 items waiting · oldest not recorded</p>"));

        let blocked = lab(
            &health(1, Some(60), false),
            &LastDelivered::default(),
            Some("Key missing: signing key."),
        );
        assert!(
            blocked.contains("<div class=\"node node--lab node--bad\">"),
            "{blocked}"
        );
        assert!(blocked.contains("Cannot deliver</span>"));
        assert!(blocked.contains("<span class=\"wire wire--cut\""));
        assert!(blocked.contains("<p class=\"node__line\">Key missing: signing key.</p>"));
        assert!(blocked.contains("<p class=\"node__line\">Nothing delivered yet · 1 item waiting"));
        assert!(blocked.contains("<strong>What to do:</strong> add the missing keys"));
        assert!(blocked.contains("href=\"/setup/lab-apps\">Add keys</a>"));
    }

    // Owner request 2026-11-19 (overview: delivery failing): items wait and
    // every try since the last delivery failed, so the lab app reads "Not
    // delivering" in red with a cut wire, and its first line says what was
    // observed; "Queued" stays for items with no failed try. Missing keys and
    // paused sending keep their own state, as no try is made then.
    #[test]
    fn failing_tries_read_not_delivering_with_what_was_observed() {
        let delivered = LastDelivered::default();
        let failing = [FailingDestination {
            destination: Identifier::new("main-lab").expect("a valid test name"),
            since: now().as_second() - 2400,
            last: now().as_second() - 60,
            tries: 74,
            next: Some(now().as_second() + 30),
        }];
        let waiting = health(2, Some(2400), false);
        let (html, observed) = lab_failing(&waiting, &delivered, &failing, None);
        assert!(
            html.contains("<div class=\"node node--lab node--bad\">"),
            "{html}"
        );
        assert!(html.contains("<span class=\"status status--bad\">"));
        assert!(html.contains("Not delivering</span>"));
        assert!(!html.contains("Queued"));
        assert!(html.contains("<span class=\"wire wire--cut\""));
        assert!(html.contains("<p class=\"node__line\">Last try <time "));
        assert!(html.contains("</time> failed · 74 failed tries since <time "));
        assert!(html.contains("</time> · next try <time "));
        assert!(html.contains(
            "<p class=\"node__line\">Nothing delivered yet · 2 items waiting · oldest <time "
        ));
        let observed = observed.expect("a to-do line for failing tries");
        assert!(observed.starts_with("Last try "), "{observed}");
        assert!(observed.contains(" failed · 74 failed tries since "));

        let other = [FailingDestination {
            destination: Identifier::new("other-lab").expect("a valid test name"),
            ..failing[0].clone()
        }];
        let (queued, none) = lab_failing(&waiting, &delivered, &other, None);
        assert!(queued.contains("Queued</span>"), "{queued}");
        assert_eq!(none, None);
        let (paused, none) = lab_failing(&health(2, None, true), &delivered, &failing, None);
        assert!(paused.contains("Sending paused</span>"), "{paused}");
        assert!(!paused.contains("Last try"));
        assert_eq!(none, None);
        let (blocked, none) = lab_failing(
            &waiting,
            &delivered,
            &failing,
            Some("Key missing: signing key."),
        );
        assert!(blocked.contains("Cannot deliver</span>"), "{blocked}");
        assert_eq!(none, None);
    }

    // Owner request 2026-11-19 (overview states): once data keeping purged
    // delivered items, a lab app with none left says so instead of "Nothing
    // delivered yet".
    #[test]
    fn purged_records_never_read_as_nothing_delivered() {
        let html = lab(
            &health(0, None, false),
            &LastDelivered {
                at: Vec::new(),
                purged: true,
            },
            None,
        );
        assert!(
            html.contains(
                "<p class=\"node__line\">No delivery in the kept records · nothing waiting</p>"
            ),
            "{html}"
        );
        assert!(!html.contains("Nothing delivered yet"));
    }

    // Owner request 2026-11-19 (overview states): the order list figure says
    // "Up to date" or "Out of date" with when it was last checked, or "Not
    // checked yet"; never a time it does not have.
    #[test]
    fn order_list_figure_says_only_what_the_health_records() {
        let name = Identifier::new("main-lab").expect("a valid test name");
        let checked = Health {
            last_successful_poll: Some(now().as_second() - 60),
            cache_trusted: true,
            ..health(0, None, false)
        };
        let fresh = figure(order_list_figure(&name, &checked, now()));
        assert!(fresh.contains("<dt>Order list · main-lab</dt>"), "{fresh}");
        assert!(fresh.contains("tone-good"));
        assert!(fresh.contains("</svg>Up to date <span class=\"figure__aside\">· checked <time "));
        let stale = figure(order_list_figure(
            &name,
            &Health {
                cache_trusted: false,
                ..checked
            },
            now(),
        ));
        assert!(stale.contains("tone-warn"), "{stale}");
        assert!(stale.contains("</svg>Out of date <span class=\"figure__aside\">· checked <time "));
        let never = figure(order_list_figure(&name, &health(0, None, false), now()));
        assert!(never.contains("tone-off"), "{never}");
        assert!(never.contains("</svg>Not checked yet</dd>"));
        assert!(!never.contains("checked <time"));
    }

    // Owner request 2026-11-19 (overview states): what waits for every lab app
    // together: the count, then the oldest item's time, "—" with "nothing
    // waiting", or "Not recorded" when the store has no age for a waiting item.
    #[test]
    fn waiting_figures_cover_none_known_and_unknown_ages() {
        let mut none = Waiting::default();
        none.add(&health(0, None, false));
        let [count, oldest] = none.figures(now());
        assert!(figure(count).contains("<dt>Items waiting to send</dt>\n<dd>0 <span class=\"figure__aside\">results and order messages</span>"));
        let oldest = figure(oldest);
        assert!(
            oldest.contains("<dd>— <span class=\"figure__aside\">nothing waiting</span></dd>"),
            "{oldest}"
        );

        let mut known = Waiting::default();
        known.add(&health(4, Some(840), false));
        let [count, oldest] = known.figures(now());
        assert!(figure(count).contains("<dd>4 "));
        let oldest = figure(oldest);
        assert!(oldest.contains("<dd><time datetime="), "{oldest}");
        assert!(oldest.contains("<span class=\"figure__aside\">14 min ago</span>"));

        let mut unknown = Waiting::default();
        unknown.add(&health(4, Some(840), false));
        unknown.add(&health(1, None, false));
        let [_, oldest] = unknown.figures(now());
        let oldest = figure(oldest);
        assert!(oldest.contains("</svg>Not recorded</dd>"), "{oldest}");
    }
}
