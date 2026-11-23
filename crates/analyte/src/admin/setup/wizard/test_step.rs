//! Starts, displays and stops the wizard's listen-only connection test.

use super::{
    super::{
        Csrf, SetupError, needs_login, off_thread,
        plan::{listen_target, plan},
        refused, see_other,
    },
    Draft, PostedForm, Rendered, Step, connection_words, foot, frame, listen,
    page::{Listen, Live, Start},
    refused_at, unread,
    verdict::{self, Family, Route},
    wizard,
};
use crate::admin::{
    server::{Connection, Context, sessions::Session},
    view::{
        Tone, format,
        parts::Hover,
        report::{Report, Section, Status, respond},
    },
};
use crate::machine::link::status::{Light, Link};
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use jiff::Timestamp;
use std::sync::Arc;

/// The one connection test and the setup choices it was started from.
pub(in crate::admin::setup) struct Test {
    pub(super) session: listen::Session,
    pub(super) machine: String,
    pub(super) draft: Draft,
    pub(super) family: Family,
    /// The model's name, beside the machine's name on the test page.
    model: String,
    /// What it listens on, for the words Analyte uses for a machine's link.
    connection: Connection,
    /// What it listens on, in words with the exact setting on hover.
    target: Hover,
    /// How it reaches the machine, for what to check when nothing arrives.
    pub(super) route: Route,
}

/// POST /setup/{id}/listen: start the listen-only test with the draft's choices.
pub(in crate::admin) async fn listen_start(
    State(state): State<Arc<Context>>,
    Path(id): Path<String>,
    Extension(session): Extension<Arc<Session>>,
    form: PostedForm,
) -> Response {
    let Ok(Form(choice)) = form else {
        return unread(&state, &session, &id);
    };
    let draft = Draft::from(choice);
    let (machine, choices) = (id.clone(), draft.clone());
    let target = off_thread(&state, move |state| {
        let plan = plan(&state.setup, &machine, &choices)?;
        let stack = plan.validated.effective().stack();
        let (family, link) = (Family::of(stack), Link::of(stack));
        let (label, target) = listen_target(&state.setup, &plan)?;
        let route = verdict::route_of(&state.setup, &target);
        Ok((label, target, route, plan, family, link))
    })
    .await;
    let (label, target, route, plan, family, link) = match target {
        Ok(found) => found,
        Err(error) => return refused_at(&state, &session, &error, &id, &draft),
    };
    let test = Test {
        session: listen::Session::start(label, target, state.setup.seams.limits),
        machine: id.clone(),
        draft,
        family,
        connection: Connection::of(link, &plan.connection),
        target: connection_words(link, &plan.connection),
        route,
        model: plan.model,
    };
    match state.setup.listen.lock() {
        // Dropping the previous test stops it and discards its bytes.
        Ok(mut slot) => *slot = Some(test),
        Err(_) => return refused(&state, Some(&session), Some(&id), &SetupError::ListenState),
    }
    see_other("/setup/listen")
}

/// GET /setup/listen: the test's state, the bytes counted, their framing
/// characters and the verdict, never the bytes themselves; reloads every two
/// seconds while running, then offers Test again. Next goes on to step 4.
pub(in crate::admin) async fn listen_view(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return needs_login(&state);
    };
    let now = Timestamp::now();
    let page = match state.setup.listen.lock() {
        Ok(slot) => slot
            .as_ref()
            .map(|test| listen_page(&state, &session, test, now)),
        // A test thread panicked while holding the state; what it held is unknown.
        Err(_) => return refused(&state, Some(&session), None, &SetupError::ListenState),
    };
    let section = Section::Setup(None);
    let Some(page) = page else {
        let title = "Connection test";
        let report = Report::new(
            section.frame(&state, title, Some(&session)),
            section,
            Tone::Neutral,
            "No test has been started",
            "Start one from a machine’s setup steps, in step 3.",
        );
        return respond(
            &state,
            Some(&session),
            section,
            StatusCode::OK,
            report.render(),
        );
    };
    respond(&state, Some(&session), section, StatusCode::OK, page)
}

/// The test page for `test` as it stands `now`.
fn listen_page(state: &Context, session: &Session, test: &Test, now: Timestamp) -> Rendered {
    let view = test.session.view();
    let limit = state.setup.seams.limits.duration.as_secs();
    let remaining = view.remaining.as_secs();
    let running = view.end.is_none();
    let mut received = format!(
        "{} received",
        format::amount(
            u64::try_from(view.seen).unwrap_or(u64::MAX),
            "byte",
            "bytes"
        )
    );
    if view.size_limit_reached {
        received.push_str(&format!(
            " · reading stopped at the {} limit",
            format::bytes(u64::try_from(state.setup.seams.limits.bytes).unwrap_or(u64::MAX))
        ));
    }
    let id = test.machine.as_str();
    Listen {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(test.model.clone()), Some(Step::Test)),
        foot: foot(id, &test.draft, true),
        live: Live {
            target: view.label.clone(),
            peer: view.peer.map(|peer| peer.to_string()),
            connects: matches!(test.route, Route::Connects(_)),
            clock: running.then(|| format::countdown(remaining)),
            elapsed: limit.saturating_sub(remaining),
            limit,
            received,
            framing: vec![if view.framing.is_empty() {
                "None yet".to_owned()
            } else {
                format::control_names(&view.framing)
            }],
            state: test_state(&view, &test.connection, id, now),
            verdict: verdict::of_view(&view, test.family, &test.route),
            stop: running.then(|| session.csrf.form_value().to_owned()),
            again: (!running).then(|| {
                start(
                    state,
                    session,
                    id,
                    &test.draft,
                    (
                        test.target.clone(),
                        matches!(test.route, Route::Connects(_)),
                    ),
                    "Test again",
                )
            }),
        },
    }
    .render()
}

/// The test's state as a status, with what Analyte observed in the words it
/// uses for a machine's link: "Listening on port 5000, nothing connected since
/// 10:41", "Port COM3 open since 10:41".
fn test_state(view: &listen::View, connection: &Connection, id: &str, now: Timestamp) -> Status {
    let link = |light, since: Timestamp| {
        format::link_sentence(connection, light, &format::clock(since, now), id)
    };
    let (tone, word, note) = match &view.end {
        None if view.closed => (
            Tone::Neutral,
            "Closed",
            format!(
                "The machine closed the connection. The test ends in {}.",
                format::duration(view.remaining.as_secs())
            ),
        ),
        None => match view.connected {
            Some(at) => (Tone::Good, "Connected", link(Light::Live, at)),
            None => (
                Tone::Attention,
                "Waiting",
                link(Light::Waiting, view.started),
            ),
        },
        Some(listen::End::TimeLimit) => (
            Tone::Neutral,
            "Ended",
            "The test ended after its time limit; the bytes were discarded.".into(),
        ),
        Some(listen::End::Stopped) => (
            Tone::Neutral,
            "Stopped",
            "The test was stopped; the bytes were discarded.".into(),
        ),
        Some(listen::End::Closed) => (
            Tone::Neutral,
            "Closed",
            "The machine closed the connection; the bytes were discarded.".into(),
        ),
        Some(listen::End::Failed(_)) => (
            Tone::Problem,
            "Failed",
            match view.connected {
                Some(_) => "Reading failed; the bytes were discarded.".into(),
                None => link(Light::Off, view.started),
            },
        ),
    };
    Status {
        tone,
        icon: tone.icon(),
        word: word.to_owned(),
        exact: String::new(),
        note: Some(note),
    }
}

/// POST /setup/listen/stop: end the test now and discard its bytes.
pub(in crate::admin) async fn listen_stop(
    State(state): State<Arc<Context>>,
    Form(_): Form<Csrf>,
) -> Response {
    if let Ok(slot) = state.setup.listen.lock()
        && let Some(test) = slot.as_ref()
    {
        test.session.stop();
    }
    see_other("/setup/listen")
}

/// The form that starts (or repeats) the listen-only test with the draft's
/// choices; `target` is what it listens on and whether Analyte connects instead.
pub(super) fn start(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
    (target, connects): (Hover, bool),
    label: &'static str,
) -> Start {
    Start {
        action: format::setup_href(id, "listen"),
        csrf: session.csrf.form_value().to_owned(),
        hidden: draft.hidden(&[]),
        target,
        connects,
        limit: state.setup.seams.limits.duration.as_secs(),
        label,
    }
}
