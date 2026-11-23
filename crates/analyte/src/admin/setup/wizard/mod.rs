//! Walks a lab manager through setting up one machine, one step per screen: 1 Machine, 2 Connection, 3 Test, 4 Save.
//!
//! No script runs, so each screen carries the earlier choices forward as hidden
//! form fields ([`Draft`]). They are never trusted: every step checks all the
//! earlier choices again against the files, and Save runs the whole check once
//! more under the write lock ([`super::plan`]). The fields hold no secret: keys
//! are set on the Lab apps page. Steps 1 to 4 only read; the connection test and
//! Save are posts with the session's CSRF value, so that value never reaches a
//! URL. Hidden fields beat a server-side draft here: there is nothing to store,
//! bound, expire or lose at a restart, and Back works in every tab.
use super::{
    SetupError, Shipped, audit, copies, definition_name, definitions, lab_apps, level_and_state,
    needs_login, off_thread,
    plan::{self, Choice as Posted, KEEP, chosen, plan, save_blocking},
    refused, running, settings_version, site,
};
use crate::admin::{
    server::{Context, sessions::Session},
    view::{
        Tab, Tone, format,
        frame::Frame,
        parts::Hover,
        report::{Button, Report, Section, respond},
    },
};
use crate::machine::link::status::Link;
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::Response,
    routing::{MethodRouter, get},
};
use jiff::Timestamp;
use page::{
    Accepts, Choice, Foot, Mark, ModelStep, Outcome, SaveStep, Sends, Summary, TestStep, Tested,
    Wizard,
};
use std::sync::Arc;

mod connection;
mod draft;
pub(super) mod listen;
mod page;
mod test_step;
pub(super) mod verdict;

use connection::render_connection;
pub(super) use connection::{connection_words, host_choices};
pub(in crate::admin) use draft::Draft;
use draft::PEERS;
pub(super) use test_step::Test;
use test_step::start;
pub(in crate::admin) use test_step::{listen_start, listen_stop, listen_view};

/// One screen of the setup steps, in step order; each is a GET with the draft as the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::admin) enum Step {
    /// GET /setup/{id}: step 1, the machine model.
    Machine,
    /// GET /setup/{id}/connection: step 2, the port or address and the lab app.
    Connection,
    /// GET /setup/{id}/test: step 3, the listen-only connection test.
    Test,
    /// GET /setup/{id}/save: step 4, the summary before saving.
    Save,
}

impl Step {
    const ALL: [Self; 4] = [Self::Machine, Self::Connection, Self::Test, Self::Save];

    const fn number(self) -> &'static str {
        match self {
            Self::Machine => "1",
            Self::Connection => "2",
            Self::Test => "3",
            Self::Save => "4",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Machine => "Machine",
            Self::Connection => "Connection",
            Self::Test => "Test",
            Self::Save => "Save",
        }
    }

    /// The button that goes back to this step to fix a refused choice.
    const fn fix(self) -> &'static str {
        match self {
            Self::Machine => "Go to step 1: Machine",
            Self::Connection => "Go to step 2: Connection",
            Self::Test => "Go to step 3: Test",
            Self::Save => "Go to step 4: Save",
        }
    }

    /// The step's page; GET, with the draft as the query.
    fn path(self, id: &str) -> String {
        format::setup_href(
            id,
            match self {
                Self::Machine => "",
                Self::Connection => "connection",
                Self::Test => "test",
                Self::Save => "save",
            },
        )
    }

    /// The step whose choice caused `error`, or `None` when no step can fix it.
    fn to_fix(error: &SetupError) -> Option<Self> {
        use crate::installation::config::Error as Installation;
        match error {
            SetupError::UnknownDefinition
            | SetupError::NoDefinitionYet
            | SetupError::BlockedDefinition(_)
            | SetupError::DefinitionFile(_)
            | SetupError::RecordOnlyModel(_)
            | SetupError::Definitions(_) => Some(Self::Machine),
            SetupError::Connection(_)
            | SetupError::UnknownChoice(_)
            | SetupError::ChoiceValue { .. }
            | SetupError::NoLabAppChosen
            | SetupError::LabAppWhileRecording
            | SetupError::RecordingAnswer(_)
            | SetupError::UnknownLabApp(_)
            | SetupError::LabAppDiffers(..)
            | SetupError::Config(_)
            | SetupError::Installation(Installation::SharedConnection { .. })
            | SetupError::InUse(_)
            | SetupError::SerialSettings(_)
            | SetupError::Lookup(_) => Some(Self::Connection),
            SetupError::NothingToListen | SetupError::RolledBack(_) => Some(Self::Save),
            SetupError::UnknownMachine(_)
            | SetupError::InvalidUrl(_)
            | SetupError::LabAppName
            | SetupError::LabAppTaken(_)
            | SetupError::KeysRequired
            | SetupError::InvalidNumber(_)
            | SetupError::LabAppInSiteFile(..)
            | SetupError::LabAppInUse(..)
            | SetupError::InvalidKey(_)
            | SetupError::IncompleteForm(_)
            | SetupError::ProgramFile(_)
            | SetupError::ProgramLayout(_)
            | SetupError::SiteFile(_)
            | SetupError::SiteDestination(_)
            | SetupError::SiteText(_)
            | SetupError::Installation(_)
            | SetupError::Io { .. }
            | SetupError::Credentials(_)
            | SetupError::KeysNotStored(_)
            | SetupError::Join(_)
            | SetupError::Audit(_)
            | SetupError::InvalidName(_)
            | SetupError::NameCharacters
            | SetupError::ReservedName(_)
            | SetupError::NameInUse(_)
            | SetupError::SiteInUse(..)
            | SetupError::HistoryNotConfirmed(_)
            | SetupError::UnfinishedWork(..)
            | SetupError::Store(_)
            | SetupError::ListenState
            | SetupError::Busy => None,
        }
    }
}

/// The stepper: every step by number and name, `current` marked and the
/// earlier ones done. `None` marks all four done, after a save.
fn marks(current: Option<Step>) -> Vec<Mark> {
    Step::ALL
        .into_iter()
        .map(|step| Mark {
            number: step.number(),
            label: step.label(),
            done: current.is_none_or(|current| step < current),
            current: current == Some(step),
        })
        .collect()
}

/// What every step shows around its form; `current` is none after a save.
fn wizard(id: &str, model: Option<String>, current: Option<Step>) -> Wizard {
    Wizard {
        id: id.to_owned(),
        model,
        steps: marks(current),
        count: format!("Step {} of 4", current.unwrap_or(Step::Save).number()),
        cancel: current.is_some(),
    }
}

/// The frame of a step about machine `id`.
fn frame(state: &Context, session: &Session, id: &str) -> Frame {
    Section::Setup(Some(id)).frame(state, format!("Set up {id}"), Some(session))
}

/// A refusal that names the step to fix and links back to it with every choice kept.
pub(super) fn refused_at(
    state: &Context,
    session: &Session,
    error: &SetupError,
    id: &str,
    draft: &Draft,
) -> Response {
    let Some(step) = Step::to_fix(error) else {
        return refused(state, Some(session), Some(id), error);
    };
    let section = Section::Setup(Some(id));
    let title = "Nothing was changed";
    let page = Report {
        lines: vec![format!(
            "Fix this in step {}: {}.",
            step.number(),
            step.label()
        )],
        links: vec![
            Button {
                href: format!("{}?{}", step.path(id), draft.query()),
                label: step.fix(),
                class: "btn--primary",
            },
            Button {
                href: Tab::Setup.href().to_owned(),
                label: "Back to Setup",
                class: "btn--outline",
            },
        ],
        ..Report::new(
            section.frame(state, title, Some(session)),
            section,
            Tone::Problem,
            "Setup could not continue",
            format::sentence(&error.to_string()),
        )
    };
    respond(
        state,
        Some(session),
        section,
        StatusCode::CONFLICT,
        page.render(),
    )
}

/// A step's page, or why its template could not be written.
type Rendered = Result<String, askama::Error>;

type Render = fn(&Context, &Session, &str, &Draft) -> Result<Rendered, SetupError>;

/// Serves one step: signed in only; files are read off the async workers; a
/// choice that does not pass is refused with the step to fix.
async fn serve(
    state: Arc<Context>,
    session: Option<Extension<Arc<Session>>>,
    id: String,
    query: Result<Query<Draft>, QueryRejection>,
    render: Render,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return needs_login(&state);
    };
    let Ok(Query(draft)) = query else {
        return refused(
            &state,
            Some(&session),
            Some(&id),
            &SetupError::IncompleteForm(START_AGAIN),
        );
    };
    let (machine, kept, owner) = (id.clone(), draft.clone(), session.clone());
    let rendered = off_thread(&state, move |state| render(state, &owner, &machine, &kept)).await;
    match rendered {
        Ok(html) => respond(
            &state,
            Some(&session),
            Section::Setup(Some(&id)),
            StatusCode::OK,
            html,
        ),
        Err(error) => refused_at(&state, &session, &error, &id, &draft),
    }
}

/// The GET handler of `step`, for the router.
pub(in crate::admin) fn step(step: Step) -> MethodRouter<Arc<Context>> {
    let render: Render = match step {
        Step::Machine => render_machine,
        Step::Connection => render_connection,
        Step::Test => render_test,
        Step::Save => render_review,
    };
    get(
        move |State(state): State<Arc<Context>>,
              session: Option<Extension<Arc<Session>>>,
              Path(id): Path<String>,
              query: Result<Query<Draft>, QueryRejection>| {
            serve(state, session, id, query, render)
        },
    )
}

fn render_machine(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let (_, installed) = site::installed(state.setup.config(), id)?;
    let list = definitions(&state.setup.seams)?;
    let recording_now = crate::installation::config::read(&installed.site)
        .is_ok_and(|text| analyte_config::recording::records_only(&text));
    let records_only = draft
        .mode
        .as_deref()
        .map_or(recording_now, |mode| mode.trim() == plan::recording::RECORD);
    let current = match chosen(&list, None, &installed, true) {
        Ok(current) => Some(current),
        Err(SetupError::NoDefinitionYet) => None,
        Err(error) => return Err(error),
    };
    let selected = draft
        .definition
        .clone()
        .or_else(|| current.as_ref().map(|model| model.form_value().to_owned()))
        .unwrap_or_default();
    let plain = |shown: String| Hover {
        shown,
        exact: String::new(),
    };
    let mut choices: Vec<Choice> = current
        .as_ref()
        .filter(|model| model.key.is_none())
        .map(|kept| Choice {
            value: KEEP.to_owned(),
            title: plain("Keep the current model".into()),
            meta: plain(kept.path.display().to_string()),
            status: None,
            checked: selected == KEEP,
            disabled: false,
        })
        .into_iter()
        .collect();
    choices.extend(list.iter().map(|definition| {
        let (checked, status, icon) = level_and_state(definition);
        Choice {
            value: definition.key.clone(),
            title: definition_name(definition),
            meta: checked,
            status: Some((status, icon)),
            checked: definition.key == selected,
            // A model not ready yet can still record, when it names its link.
            disabled: !matches!(
                definition.state,
                Shipped::Ready(_) | Shipped::Blocked { record: Some(_) }
            ),
        }
    }));
    Ok(ModelStep {
        frame: frame(state, session, id),
        wizard: wizard(id, current.map(|model| model.name), Some(Step::Machine)),
        action: Step::Connection.path(id),
        hidden: draft.hidden(&["definition", "mode"]),
        choices,
        records_only,
    }
    .render())
}

fn render_test(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let plan = plan(&state.setup, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let folder = link == Link::Folder;
    let start = (!folder).then(|| {
        start(
            state,
            session,
            id,
            draft,
            (
                connection_words(link, &plan.connection),
                link == Link::TcpClient,
            ),
            "Start the test",
        )
    });
    Ok(TestStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(plan.model), Some(Step::Test)),
        foot: foot(id, draft, folder),
        start,
    }
    .render())
}

/// Back to step 2 and Next to step 4, both carrying the draft.
fn foot(id: &str, draft: &Draft, primary: bool) -> Foot {
    Foot {
        back: Step::Connection.path(id),
        next: Step::Save.path(id),
        hidden: draft.hidden(&[]),
        primary,
    }
}

fn render_review(
    state: &Context,
    session: &Session,
    id: &str,
    draft: &Draft,
) -> Result<Rendered, SetupError> {
    let plan = plan(&state.setup, id, draft)?;
    let link = Link::of(plan.validated.effective().stack());
    let destination = &plan.validated.effective().site().destination;
    let summary = Summary {
        connection: connection_words(link, &plan.connection),
        cable: plan.cable(),
        choices: plan.choices(),
        accepts: (link == Link::TcpServer)
            .then(|| offer(plan.peers.as_ref(), seen(state, id, draft))),
        lab_app: destination.as_ref().map(|destination| {
            sends(
                destination,
                lab_apps::missing_keys(&state.setup, destination),
            )
        }),
        test: tested(state, id, draft, link),
        versions: None,
        model: plan.model.clone(),
    };
    Ok(SaveStep {
        frame: frame(state, session, id),
        wizard: wizard(id, Some(plan.model), Some(Step::Save)),
        summary,
        outcome: Outcome::Review {
            back: Step::Test.path(id),
            back_hidden: draft.hidden(&[]),
            action: format::setup_href(id, ""),
            csrf: session.csrf.form_value().to_owned(),
            // The allowed devices come from the step's tick box instead.
            hidden: draft.hidden(&[PEERS]),
        },
    }
    .render())
}

/// What step 4 offers about the devices a listening machine accepts (ADR
/// 0026): the typed addresses, or else the device the connection test saw,
/// ticked by default; with neither, a warning that any device is accepted.
fn offer(typed: Option<&analyte_config::AllowedPeers>, seen: Option<std::net::IpAddr>) -> Accepts {
    match (typed, seen) {
        (Some(typed), seen) => Accepts::Offer {
            value: typed.to_string(),
            label: if typed.addresses().len() == 1 {
                format!("Only accept this device: {typed}")
            } else {
                format!("Only accept these devices: {typed}")
            },
            note: seen.filter(|seen| !typed.allows(*seen)).map(|seen| {
                format!("The connection test saw {seen}, which is not on this list.")
            }),
        },
        (None, Some(seen)) => Accepts::Offer {
            value: seen.to_string(),
            label: format!("Only accept this device: {seen}"),
            note: Some(
                "The device that connected during the test. Check that this is the machine’s own address in its network settings."
                    .into(),
            ),
        },
        (None, None) => Accepts::Any,
    }
}

/// The device the last connection test for `id` saw connect, when it ran
/// with exactly these choices and Analyte listened.
fn seen(state: &Context, id: &str, draft: &Draft) -> Option<std::net::IpAddr> {
    let slot = state.setup.listen.lock().ok()?;
    slot.as_ref()
        .filter(|test| test.machine == id && test.draft.same(draft))
        .and_then(|test| test.session.view().peer)
}

/// The lab app the machine sends to, with the keys it still lacks.
fn sends(destination: &analyte_config::Destination, missing: Option<String>) -> Sends {
    Sends {
        name: destination.name.as_str().to_owned(),
        href: format::lab_app_href(destination.name.as_str()),
        address: destination.base_url.as_str().to_owned(),
        missing,
    }
}

/// The verdict of the last connection test for `id`, when it ran with exactly
/// these choices; a watched folder has nothing to test.
fn tested(state: &Context, id: &str, draft: &Draft, link: Link) -> Tested {
    if link == Link::Folder {
        return Tested::Folder;
    }
    let Ok(slot) = state.setup.listen.lock() else {
        return Tested::NotRun;
    };
    match slot
        .as_ref()
        .filter(|test| test.machine == id && test.draft.same(draft))
    {
        Some(test) => {
            let view = test.session.view();
            let verdict = verdict::of_view(&view, test.family, &test.route);
            Tested::Seen {
                tone: verdict.tone,
                title: verdict.title,
                when: format::moment(view.started, Timestamp::now()),
            }
        }
        None => Tested::NotRun,
    }
}

/// A posted step (the test or Save), or why its form could not be read.
type PostedForm = Result<Form<Posted>, axum::extract::rejection::FormRejection>;

/// What to do after a step's form could not be read.
const START_AGAIN: &str = "start again from the setup page";

/// The refusal of a step's form that could not be read.
fn unread(state: &Context, session: &Session, id: &str) -> Response {
    refused(
        state,
        Some(session),
        Some(id),
        &SetupError::IncompleteForm(START_AGAIN),
    )
}

/// POST /setup/{id}: check every choice again, write the files with backups, show what was saved.
pub(in crate::admin) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: PostedForm,
) -> Response {
    let Ok(Form(choice)) = form else {
        return unread(&state, &session, &id);
    };
    let draft = Draft::from(choice);
    let (machine, kept) = (id.clone(), draft.clone());
    let saved = off_thread(&state, move |state| {
        let saved = save_blocking(&state.setup, &machine, &kept)?;
        let missing = saved
            .lab_app
            .as_ref()
            .and_then(|lab_app| lab_apps::missing_keys(&state.setup, lab_app));
        Ok((saved, missing))
    })
    .await;
    let (saved, missing) = match saved {
        Ok(saved) => saved,
        Err(error) => return refused_at(&state, &session, &error, &id, &draft),
    };
    let reason = saved.reason(super::Via::Page);
    let audited = audit(&state, &session, "setup-save", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page saved a site file");
    let mut lines = Vec::new();
    if saved.moved {
        lines.push(
            "This lab app is now kept on the Lab apps page; nothing else changed.".to_owned(),
        );
    }
    if !saved.backups.is_empty() {
        lines.push("A dated copy of the old settings was kept.".into());
    }
    let page = SaveStep {
        frame: frame(&state, &session, &id),
        wizard: wizard(&id, Some(saved.model.clone()), None),
        summary: Summary {
            connection: connection_words(saved.link, &saved.connection),
            cable: saved.cable.clone(),
            choices: saved.choices.clone(),
            accepts: (saved.link == Link::TcpServer).then(|| {
                Accepts::Saved(
                    saved.peers.as_ref().map_or_else(
                        || format::OPEN_TO_ANY.into(),
                        |peers| format!("Only {peers}"),
                    ),
                    saved.peers.is_none(),
                )
            }),
            lab_app: saved
                .lab_app
                .as_ref()
                .map(|lab_app| sends(lab_app, missing)),
            test: tested(&state, &id, &draft, saved.link),
            versions: Some((
                Hover {
                    shown: format::digest_short(&saved.digest),
                    exact: format::digest_hex(&saved.digest),
                },
                settings_version(running(&state, &id)),
            )),
            model: saved.model,
        },
        outcome: Outcome::Saved {
            csrf: session.csrf.form_value().to_owned(),
            lines,
            problems: audited
                .err()
                .map(|error| format::sentence(&error.to_string()))
                .into_iter()
                .collect(),
            support: copies(&saved.backups),
            done: format::machine_href(&id),
        },
    };
    respond(
        &state,
        Some(&session),
        Section::Setup(Some(&id)),
        StatusCode::OK,
        page.render(),
    )
}

#[cfg(test)]
mod tests {
    use super::{Draft, Step, marks};

    // Owner request 2026-11-19 (item 3): the stepper lists the four steps,
    // marks the current one and the earlier ones done; after a save all four
    // are done and none is current.
    #[test]
    fn stepper_marks_the_current_step_and_the_done_ones() {
        let shown: Vec<(&str, &str, bool, bool)> = marks(Some(Step::Test))
            .iter()
            .map(|mark| (mark.number, mark.label, mark.done, mark.current))
            .collect();
        assert_eq!(
            shown,
            [
                ("1", "Machine", true, false),
                ("2", "Connection", true, false),
                ("3", "Test", false, true),
                ("4", "Save", false, false),
            ]
        );
        let saved = marks(None);
        assert!(saved.iter().all(|mark| mark.done && !mark.current));
    }

    // Owner request 2026-11-19 (item 3): hidden fields carry the choices, a
    // step leaves out the ones it shows itself, and a refusal's link carries
    // them all as an encoded query.
    #[test]
    fn hidden_fields_skip_shown_ones_and_links_encode_them() {
        let draft = Draft {
            definition: Some("example/serial-astm".into()),
            port: Some("usb:\"><script>".into()),
            lab_app: Some("demo".into()),
            ..Draft::default()
        };
        assert_eq!(
            draft.hidden(&["port"]),
            [
                ("definition".to_owned(), "example/serial-astm".to_owned()),
                ("lab_app".to_owned(), "demo".to_owned())
            ]
        );
        assert_eq!(
            draft.query(),
            "definition=example%2Fserial-astm&port=usb%3A%22%3E%3Cscript%3E&lab_app=demo"
        );
    }
}
