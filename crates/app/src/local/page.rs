//! Builds the Engine off screen from what the app observed: when it last reached the engine, the state it saw then, what the last question and the service manager said, and the last start request.
//!
//! It never states what the app did not see: no machine or result counts,
//! which only the signed-in pages know.
use crate::engine::service::{Service, Started};
use crate::engine::status::{Health, Reply};
use analyte_i18n::{message as tr_args, text as tr};
use askama::Template as _;
use jiff::{Timestamp, Zoned, tz::TimeZone};

/// What the app observed, as the Engine off screen shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Off {
    /// The engine's origin, such as `http://127.0.0.1:8080`.
    pub origin: String,
    /// When the engine last answered, and the state it gave then.
    pub last_answer: Option<(Timestamp, Health)>,
    /// What the last question to `/status` brought back.
    pub reply: Option<Reply>,
    /// When the app noticed the engine was not answering.
    pub noticed: Option<Timestamp>,
    /// What the service manager said about the service then.
    pub service: Option<Service>,
    /// What the last start request came to.
    pub started: Option<Started>,
    /// A start request is running now.
    pub starting: bool,
    /// Why the app's settings file could not be used; then no engine is asked.
    pub settings: Option<String>,
}

/// The Start engine form; `busy` while a request runs.
pub(super) struct Start {
    pub(super) busy: bool,
}

#[derive(askama::Template)]
#[template(path = "engine-off.html")]
struct Page {
    engine_title: &'static str,
    noticed: String,
    title: &'static str,
    lead: String,
    facts: Vec<(&'static str, String)>,
    start: Option<Start>,
}

/// The screen as HTML, with every value escaped.
///
/// # Errors
///
/// When the template cannot be written.
pub(super) fn render(off: &Off, zone: &TimeZone, now: Timestamp) -> Result<String, askama::Error> {
    let stopped = matches!(off.service, Some(Service::Stopped));
    let (engine_title, title) = if off.settings.is_some() {
        (
            tr("desktop-engine-unknown"),
            tr("desktop-the-app-cannot-find-the-engine"),
        )
    } else {
        match off.service {
            Some(Service::Stopped) => (
                tr("desktop-engine-stopped"),
                tr("desktop-the-analyte-engine-isn-t-running"),
            ),
            Some(Service::Starting) => (
                tr("desktop-engine-starting"),
                tr("desktop-the-analyte-engine-is-starting"),
            ),
            _ => (
                tr("desktop-not-answering"),
                tr("desktop-the-analyte-engine-isn-t-answering"),
            ),
        }
    };
    let lead = if let Some(why) = &off.settings {
        tr_args(
            "desktop-the-app-s-settings-file-could-not-be-used",
            [("why", (why).to_string().into())],
        )
    } else if stopped {
        tr("desktop-machines-aren-t-being-listened-to-and-nothing-is").to_owned()
    } else {
        tr_args(
            "desktop-engine-no-answer",
            [("address", off.origin.as_str().into())],
        )
    };
    let offer = off.settings.is_none()
        && matches!(
            off.service,
            Some(Service::Stopped | Service::Other | Service::Unreadable(_))
        );
    Page {
        engine_title,
        noticed: off
            .noticed
            .map_or_else(|| "—".to_owned(), |at| clock(at, zone)),
        title,
        lead,
        facts: facts(off, zone, now),
        start: offer.then_some(Start { busy: off.starting }),
    }
    .render()
}

/// The observed facts, one row each.
fn facts(off: &Off, zone: &TimeZone, now: Timestamp) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        (
            "Last reached",
            off.last_answer.map_or_else(
                || "Not since the app started".to_owned(),
                |(at, _)| moment(at, zone, now),
            ),
        ),
        (
            tr("desktop-last-known-state"),
            off.last_answer.map_or_else(
                || tr("admin-nothing-yet").to_owned(),
                |(_, health)| health.words().to_owned(),
            ),
        ),
    ];
    let mut saw = Vec::new();
    match &off.reply {
        Some(Reply::Silent(silence)) => saw.push(silence.words().to_owned()),
        Some(Reply::Unrecognised(why)) => saw.push(tr_args(
            "desktop-another-program-answers-at-the-engine-s-address",
            [("why", (why).to_string().into())],
        )),
        Some(Reply::Answered(..)) | None => {}
    }
    if let Some(service) = &off.service {
        saw.push(service.words());
    }
    if !saw.is_empty() {
        rows.push((tr("desktop-what-the-app-saw"), saw.join(" ")));
    }
    if off.starting {
        rows.push((
            tr("desktop-start-engine"),
            tr("desktop-asking-windows-to-start-the-engine").to_owned(),
        ));
    } else if let Some(started) = &off.started {
        rows.push((tr("desktop-start-engine"), started.words()));
    }
    rows
}

/// A time on this computer's clock, such as `10:52`.
fn clock(at: Timestamp, zone: &TimeZone) -> String {
    Zoned::new(at, zone.clone()).strftime("%H:%M").to_string()
}

/// A time with its day: `10:52 today`, or `19 Nov 10:52` on another day.
fn moment(at: Timestamp, zone: &TimeZone, now: Timestamp) -> String {
    let then = Zoned::new(at, zone.clone());
    let today = Zoned::new(now, zone.clone());
    if then.date() == today.date() {
        tr_args(
            "desktop-clock-today",
            [("time", then.strftime("%H:%M").to_string().into())],
        )
    } else {
        then.strftime("%-d %b %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::status::Silence;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).expect("a valid test time")
    }

    fn page(off: &Off) -> String {
        render(off, &TimeZone::UTC, at(1_763_600_000)).expect("the screen renders")
    }

    // EngineOff mockup, ADR 0019: a stopped service gets the mockup's title
    // and Start engine; the facts are only what the app observed.
    #[test]
    fn stopped_service_offers_start_and_states_only_what_was_seen() {
        let html = page(&Off {
            origin: "http://127.0.0.1:8080".into(),
            last_answer: Some((at(1_763_599_920), Health::Attention)),
            reply: Some(Reply::Silent(Silence::Refused)),
            noticed: Some(at(1_763_599_990)),
            service: Some(Service::Stopped),
            ..Off::default()
        });
        assert!(html.contains(
            "<h1 id=\"eo-title\" class=\"panel__title\">The Analyte engine isn’t running</h1>"
        ));
        assert!(
            html.contains("<dt>Last reached</dt><dd>00:52 today</dd>"),
            "{html}"
        );
        assert!(html.contains("<dt>Last known state</dt><dd>Something needs you</dd>"));
        assert!(html.contains("The Analyte service is stopped."));
        assert!(html.contains(
            "<button type=\"submit\" class=\"btn btn--primary btn--lg\">Start engine</button>"
        ));
        assert!(html.contains("<span class=\"engine__meta\">Noticed 00:53</span>"));
        for unseen in ["machines connected", "results waiting", "<script", "style="] {
            assert!(!html.contains(unseen), "{unseen}");
        }
    }

    // ADR 0019: without a stopped service there is nothing to start; the
    // screen says the engine is not answering and where it asked.
    #[test]
    fn a_running_service_that_does_not_answer_offers_no_start() {
        let html = page(&Off {
            origin: "http://127.0.0.1:8080".into(),
            reply: Some(Reply::Silent(Silence::TimedOut)),
            service: Some(Service::Running),
            ..Off::default()
        });
        assert!(html.contains("The Analyte engine isn’t answering"));
        assert!(html.contains("at http://127.0.0.1:8080."));
        assert!(html.contains("<dd>Not since the app started</dd>"));
        assert!(html.contains("<dd>Nothing yet</dd>"));
        assert!(!html.contains("Start engine</button>"));
        assert!(!html.contains("If it doesn’t start"));
        assert!(html.contains("The app keeps checking and updates this screen by itself."));
    }

    // ADR 0019: while a start request runs the button is disabled; its
    // outcome is shown afterwards in plain words.
    #[test]
    fn start_request_and_outcome_are_shown() {
        let busy = page(&Off {
            service: Some(Service::Stopped),
            starting: true,
            ..Off::default()
        });
        assert!(busy.contains("disabled>Starting…</button>"));
        let denied = page(&Off {
            service: Some(Service::Stopped),
            started: Some(Started::NotAllowed),
            ..Off::default()
        });
        assert!(denied.contains("Your Windows account may not start the engine."));
    }

    // AGENTS.md (keep errors specific): a broken settings file is said as it
    // is, and nothing is offered.
    #[test]
    fn settings_error_is_shown_and_nothing_offered() {
        let html = page(&Off {
            settings: Some("analyte-app.toml gives admin port 0, which is not a port".into()),
            service: Some(Service::Stopped),
            ..Off::default()
        });
        assert!(html.contains("The app cannot find the engine"));
        assert!(html.contains("admin port 0"));
        assert!(!html.contains("Start engine</button>"));
    }

    // Safety rule: every value is escaped.
    #[test]
    fn values_are_escaped() {
        let html = page(&Off {
            origin: "<b>x</b>".into(),
            ..Off::default()
        });
        assert!(html.contains("&#60;b&#62;x&#60;/b&#62;") || html.contains("&lt;b&gt;x"));
    }
}
