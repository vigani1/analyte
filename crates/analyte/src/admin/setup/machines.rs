//! Adds and removes machines on the setup page: the forms, the checks, the warnings and the audit.
//!
//! Both write through the same checked commit as Save (backup, safe write, reload,
//! rollback) and apply after a restart. Removing never touches the store: a
//! removed machine's receipts, flagged items and history stay listed for staff.
//! Its lab app stays in `lab-apps.toml` (moved there first if the site file
//! still wrote it), so results waiting for it are still delivered. A machine just
//! added goes straight into step 1 of the setup steps.
use super::{
    RELOAD, Setup, SetupError, Via, audit, copies, entries, needs_login, off_thread, page, refused,
    see_other, site, stored,
};
use crate::admin::machines::{self, DialogItem, RemoveDialog};
use crate::admin::view::parts::Hover;
use crate::admin::view::report::{Button, Problem, Report, Section, respond};
use crate::admin::view::{Tone, icon::Icon};
use crate::admin::{
    server::{Context, sessions::Session},
    view::format,
};
use crate::installation::config::{Program, folder};
use analyte_model::Identifier;
use analyte_store::Unfinished;
use askama::Template as _;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

/// GET /setup/add: the form for one more machine, with the next free name filled in.
pub(in crate::admin) async fn add_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return needs_login(&state);
    };
    let suggested = off_thread(&state, |state| {
        site::entries(state.setup.config()).map(|(_, listed)| entries::suggestion(&listed))
    })
    .await;
    let name = match suggested {
        Ok(name) => name,
        Err(error) => return refused(&state, Some(&session), None, &error),
    };
    let section = Section::Setup(None);
    let page = page::Add {
        frame: section.frame(
            &state,
            crate::admin::view::format::ADD_MACHINE,
            Some(&session),
        ),
        head: section.head(crate::admin::view::format::ADD_MACHINE),
        csrf: session.csrf.form_value().to_owned(),
        name,
    };
    respond(
        &state,
        Some(&session),
        section,
        StatusCode::OK,
        page.render(),
    )
}

/// The add form as posted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Addition {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    #[serde(default)]
    name: String,
    same_analyzer: Option<String>,
}

/// Two entries name the same file; file names on Windows ignore case.
fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

fn add_blocking(
    setup: &Setup,
    id: &Identifier,
    serial: Option<&analyte_config::SerialValues>,
) -> Result<(PathBuf, site::Written), SetupError> {
    let _writing = setup.writing()?;
    let config = setup.config();
    let (program, listed) = site::entries(config)?;
    if entries::taken(&listed, id.as_str()) {
        return Err(SetupError::NameInUse(id.as_str().into()));
    }
    let file = entries::site_file(id);
    let folder = folder(config);
    let path = folder.join(&file);
    if let Some(owner) = listed
        .iter()
        .find(|entry| same_path(&folder.join(entry.site.get_ref()), &path))
    {
        return Err(SetupError::SiteInUse(file, owner.id.get_ref().clone()));
    }
    let edited = entries::added(&program, &listed, id, &file)?;
    let written = site::commit(
        config,
        &[
            (path.clone(), entries::starter_site(id, serial)?),
            (config.to_owned(), edited),
        ],
    )?;
    Ok((path, written))
}

/// A machine just added to the program file with its starter site file.
pub(super) struct Added {
    pub(super) id: Identifier,
    /// Its starter site file.
    pub(super) path: PathBuf,
    pub(super) written: site::Written,
    /// It joins the stored history of an earlier machine with this name.
    pub(super) reused: bool,
}

impl Added {
    /// The audit reason: what was added, by whom, and whether it joins a history.
    pub(super) fn reason(&self, via: Via) -> String {
        format!(
            "{} added machine {} with starter site file {}{}",
            via.machines(),
            self.id.as_str(),
            self.path.display(),
            if self.reused {
                "; joins that name's stored history (same analyzer confirmed)"
            } else {
                ""
            }
        )
    }
}

/// Checks the name, refuses a listed one, asks for `same_analyzer` when the
/// name has stored history, then adds the entry and its starter site file,
/// with the serial cable settings `serial` when given (ADR 0031).
pub(super) async fn add_machine(
    setup: &Arc<Setup>,
    store: &analyte_store::Handle,
    name: &str,
    same_analyzer: bool,
    serial: Option<analyte_config::SerialValues>,
) -> Result<Added, SetupError> {
    let id = entries::name(name)?;
    if let Some(serial) = &serial {
        serial.check(&entries::site_file(&id))?;
    }
    // A listed name is refused as such before its history is looked at;
    // `add_blocking` checks again under the write lock.
    let wanted = id.clone();
    let taken = off_thread(setup, move |setup| {
        site::entries(setup.config()).map(|(_, listed)| entries::taken(&listed, wanted.as_str()))
    })
    .await?;
    if taken {
        return Err(SetupError::NameInUse(id.as_str().into()));
    }
    let reused = history(store, &id).await?;
    if reused && !same_analyzer {
        return Err(SetupError::HistoryNotConfirmed(id.as_str().into()));
    }
    let machine = id.clone();
    let (path, written) = off_thread(setup, move |setup| {
        add_blocking(setup, &machine, serial.as_ref())
    })
    .await?;
    Ok(Added {
        id,
        path,
        written,
        reused,
    })
}

/// POST /setup/add: check the name, add the entry and its starter site file.
pub(in crate::admin) async fn add(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    form: Result<Form<Addition>, axum::extract::rejection::FormRejection>,
) -> Response {
    let refuse = |error: &SetupError| refused(&state, Some(&session), None, error);
    let Ok(Form(addition)) = form else {
        return refuse(&SetupError::IncompleteForm(RELOAD));
    };
    let same = addition.same_analyzer.as_deref() == Some("yes");
    let added = match add_machine(&state.setup, &state.store, &addition.name, same, None).await {
        Ok(added) => added,
        Err(error) => return refuse(&error),
    };
    let id = added.id.as_str();
    let audited = audit(&state, &session, "setup-add", id, added.reason(Via::Page)).await;
    tracing::info!(machine = id, "setup page added a machine");
    // Straight into step 1, unless the audit failed: that is shown, never hidden.
    match audited {
        Ok(()) => see_other(&format::setup_href(id, "")),
        Err(error) => added_page(
            &state,
            &session,
            id,
            added.reused,
            &added.path,
            &added.written,
            &error,
        ),
    }
}

/// What adding a machine did when the record of who added it could not be
/// written: the machine is listed, the failure is shown, then Restart now.
fn added_page(
    state: &Context,
    session: &Session,
    id: &str,
    reused: bool,
    path: &std::path::Path,
    written: &site::Written,
    error: &SetupError,
) -> Response {
    let section = Section::Setup(Some(id));
    let mut lines = vec!["It is listed and not set up yet.".to_owned()];
    if reused {
        lines.push(crate::admin::setup::REUSED_MACHINE_HISTORY.into());
    }
    if !written.backups.is_empty() {
        lines.push(crate::admin::setup::BACKUP_KEPT.into());
    }
    let mut support = vec![format!("Its settings file: {}", path.display())];
    support.extend(copies(&written.backups));
    let page = Report {
        lines,
        problems: vec![format::sentence(&error.to_string())],
        restart: Some(session.csrf.form_value().to_owned()),
        links: vec![
            Button {
                href: format::setup_href(id, ""),
                label: "Set it up now",
                class: "btn--primary",
            },
            Button {
                href: crate::admin::view::Tab::Setup.href().to_owned(),
                label: crate::admin::view::Tab::Setup.back(),
                class: "btn--outline",
            },
        ],
        support,
        ..Report::new(
            section.frame(state, "Machine added", Some(session)),
            section,
            Tone::Good,
            format!("{id} is added"),
            "Set it up now, or restart first.",
        )
    };
    respond(state, Some(session), section, StatusCode::OK, page.render())
}

/// GET /setup/first: the first-run button. Step 1 for the first listed machine
/// that is not set up yet; Add a machine when none is listed; otherwise the setup page.
pub(in crate::admin) async fn first(State(state): State<Arc<Context>>) -> Response {
    let listed = off_thread(&state, |state| {
        site::entries(state.setup.config()).map(|(_, listed)| {
            let waiting = listed
                .iter()
                .find(|entry| entry.definition.is_none())
                .map(|entry| entry.id.get_ref().clone());
            (waiting, listed.is_empty())
        })
    })
    .await;
    match listed {
        Ok((Some(id), _)) => see_other(&format!("/setup/{id}")),
        Ok((None, true)) => see_other("/setup/add"),
        Ok((None, false)) => see_other("/setup"),
        Err(error) => refused(&state, None, None, &error),
    }
}

async fn history(store: &analyte_store::Handle, id: &Identifier) -> Result<bool, SetupError> {
    let machine = id.clone();
    stored::<_, analyte_store::OperatorError>(store, move |store| store.has_history(&machine))
        .await
        .map_err(SetupError::Store)
}

/// What is still open for machine `id` in the store.
pub(super) async fn unfinished(
    store: &analyte_store::Handle,
    id: &str,
) -> Result<Unfinished, SetupError> {
    let machine = Identifier::new(id).map_err(SetupError::InvalidName)?;
    stored::<_, analyte_store::OperatorError>(store, move |store| store.unfinished(&machine))
        .await
        .map_err(SetupError::Store)
}

/// One line per kind of open work, or none when nothing is open.
fn open_work(open: &Unfinished) -> Vec<String> {
    [
        (
            open.undelivered,
            "result or order message",
            "results or order messages",
            " not yet accepted by the lab app",
        ),
        (
            open.open_orders,
            "order",
            "orders",
            concat!(
                " sent to the machine and not finished (sending, sent, accepted, ",
                "cancel requested or uncertain); once it is removed nothing serves them, so ",
                "reconcile them with the machine first (Machine orders page)"
            ),
        ),
        (
            open.unread,
            "received message",
            "received messages",
            " not read yet; only this machine's settings read them, at its next start",
        ),
        (
            open.flagged,
            "item waiting for review",
            "items waiting for review",
            " linked to this machine (Needs review page)",
        ),
    ]
    .into_iter()
    .filter(|(count, ..)| *count > 0)
    .map(|(count, one, many, rest)| format!("{}{rest}", format::amount(count, one, many)))
    .collect()
}

/// The lab app machine `id` is configured with (overrides included), which
/// removing keeps in `lab-apps.toml`; none while it waits for setup. An error
/// says why it cannot be read, and then nothing is kept.
fn configured_destination(
    config: &std::path::Path,
    id: &str,
) -> Result<Option<analyte_config::Destination>, crate::installation::config::Error> {
    let program = Program::load(config)?;
    Ok(program
        .machines()
        .iter()
        .find(|machine| machine.id().as_str() == id)
        .and_then(|machine| machine.snapshot().effective().site().destination.clone()))
}

/// The lab app a machine sends to, as far as removing it is concerned.
enum LabApp {
    /// Not set up: it has no lab app.
    None,
    /// Its settings cannot be read, so its lab app cannot be kept.
    Unreadable(crate::installation::config::Error),
    /// The lab app stays on the Lab apps page (moved into `lab-apps.toml` first if
    /// the site file still wrote it), and the service sends to it until nothing
    /// is left (`runtime::delivery::unused_lab_apps`).
    Kept(String),
}

fn lab_app(config: &std::path::Path, id: &str) -> LabApp {
    match configured_destination(config, id) {
        Ok(Some(destination)) => LabApp::Kept(destination.name.as_str().to_owned()),
        Ok(None) => LabApp::None,
        Err(error) => LabApp::Unreadable(error),
    }
}

/// One sentence on what happens to the machine's waiting results after
/// removal, with their exact count on hover when some wait.
fn delivery_sentence(lab_app: &LabApp, open: &Result<Unfinished, SetupError>) -> Hover {
    let plain = |shown: String| Hover {
        shown,
        exact: String::new(),
    };
    match lab_app {
        LabApp::None => plain("It is not set up, so nothing is waiting to be sent.".into()),
        LabApp::Unreadable(error) => plain(format!(
            concat!(
                "Its lab app settings cannot be read ({}), so results still waiting for it are ",
                "kept, never deleted, until that lab app is added on the Lab apps page."
            ),
            error
        )),
        LabApp::Kept(name) => match open {
            Ok(open) if open.undelivered > 0 => Hover {
                shown: format!(
                    "Its {} will still be sent to {name}.",
                    format::amount(open.undelivered, "waiting result", "waiting results")
                ),
                exact: format::amount(
                    open.undelivered,
                    "result or order message not yet accepted by the lab app",
                    "results or order messages not yet accepted by the lab app",
                ),
            },
            Ok(_) => plain(format!(
                "Nothing is waiting; anything it sends before the restart will still be sent to {name}."
            )),
            Err(_) => plain(format!("Its waiting results will still be sent to {name}.")),
        },
    }
}

/// What removing a machine would leave behind, as the Remove dialog and the
/// `machines remove` command say it.
pub(super) struct Preview {
    /// What happens to its waiting results.
    pub(super) sending: Hover,
    /// What is still unfinished, then what removing keeps.
    pub(super) items: Vec<DialogItem>,
    /// Something is unfinished or could not be counted, so removing needs "anyway".
    pub(super) warned: bool,
}

/// Reads what is still unfinished for machine `id` and what removing it keeps.
pub(super) async fn preview_removal(
    setup: &Arc<Setup>,
    store: &analyte_store::Handle,
    id: &str,
) -> Result<Preview, SetupError> {
    let machine = id.to_owned();
    let (kept, last) = off_thread(setup, move |setup| {
        site::installed(setup.config(), &machine)?;
        let kept = lab_app(setup.config(), &machine);
        let (_, listed) = site::entries(setup.config())?;
        Ok((kept, listed.len() == 1))
    })
    .await?;
    let open = unfinished(store, id).await;
    let (mut items, warned) = warning(&open);
    if last {
        items.push(DialogItem {
            tone: Tone::Attention,
            icon: Icon::Warning,
            text: concat!(
                "This is the last machine. After the restart Analyte serves only this admin ",
                "page until a machine is added."
            )
            .into(),
        });
    }
    items.push(DialogItem {
        tone: Tone::Neutral,
        icon: Icon::Ok,
        text: crate::admin::setup::BACKUP_ADVICE.into(),
    });
    Ok(Preview {
        sending: delivery_sentence(&kept, &open),
        items,
        warned,
    })
}

/// GET /setup/{id}/remove: the machine's page with the Remove dialog over it,
/// saying what is still unfinished.
pub(in crate::admin) async fn confirm_remove(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.is_signed_in()) else {
        return Problem::SignIn.page(&state, None, Section::Machines(Some(&id)));
    };
    let preview = match preview_removal(&state.setup, &state.store, &id).await {
        Ok(preview) => preview,
        Err(error) => {
            return Problem::Refused(error.to_string()).page(
                &state,
                Some(&session),
                Section::Machines(Some(&id)),
            );
        }
    };
    let warned = preview.warned;
    let dialog = RemoveDialog {
        action: format::setup_href(&id, "remove"),
        csrf: session.csrf.form_value().to_owned(),
        cancel: format::machine_href(&id),
        title: format!("Remove {id}?"),
        text: format::REMOVED_ADVICE,
        sending: preview.sending,
        list_title: if warned {
            "Still unfinished"
        } else {
            crate::admin::setup::BEFORE_REMOVAL
        },
        items: preview.items,
        anyway: warned,
        button: Some(if warned {
            "Remove anyway"
        } else {
            crate::admin::setup::REMOVE
        }),
    };
    machines::with_dialog(&state, &session, &id, dialog).await
}

/// The dialog's lines about open work, or why it could not be counted; true
/// when there is a warning, so removing needs "Remove anyway".
fn warning(open: &Result<Unfinished, SetupError>) -> (Vec<DialogItem>, bool) {
    const UNCOUNTED: &str = "Items to review that are not linked to one machine are not counted.";
    match open {
        Ok(open) if open.is_empty() => (
            vec![
                DialogItem {
                    tone: Tone::Good,
                    icon: Icon::Ok,
                    text: "Nothing is unfinished for this machine.".into(),
                },
                DialogItem {
                    tone: Tone::Neutral,
                    icon: Icon::Info,
                    text: UNCOUNTED.into(),
                },
            ],
            false,
        ),
        Ok(open) => {
            let mut items: Vec<DialogItem> = open_work(open)
                .into_iter()
                .map(|line| DialogItem {
                    tone: Tone::Attention,
                    icon: Icon::Warning,
                    text: line,
                })
                .collect();
            items.push(DialogItem {
                tone: Tone::Neutral,
                icon: Icon::Info,
                text: format!(
                    "Removing does not delete any of this. Finish it first, or remove anyway. {UNCOUNTED}"
                ),
            });
            (items, true)
        }
        Err(error) => (
            vec![DialogItem {
                tone: Tone::Problem,
                icon: Icon::Problem,
                text: format!(
                    "Unfinished work could not be counted: {error}. Check the Deliveries, Machine orders and Needs review pages first."
                ),
            }],
            true,
        ),
    }
}

/// The remove form as posted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Removal {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    anyway: Option<String>,
}

/// What removing a machine changed.
pub(super) struct Removed {
    pub(super) backups: Vec<PathBuf>,
    /// The lab app kept for delivery, none for a machine not set up, or why its
    /// settings could not be read (then nothing is kept).
    pub(super) kept: Result<Option<String>, crate::installation::config::Error>,
    /// Where the site file went: a dated copy, nowhere (missing or still named by
    /// another machine), or why moving it failed after the program was changed.
    pub(super) site: Result<Option<PathBuf>, SetupError>,
    /// What was unfinished at removal, in words.
    pub(super) summary: String,
    pub(super) anyway: bool,
}

impl Removed {
    /// Where its site file went, in one sentence.
    pub(super) fn site_sentence(&self) -> String {
        match &self.site {
            Ok(Some(path)) => format!(
                "Its settings file was kept as a dated copy: {}",
                path.display()
            ),
            Ok(None) => {
                "Its settings file was left in place (missing or used by another machine)".into()
            }
            Err(error) => format!("Its settings file could not be moved: {error}"),
        }
    }

    /// What happens to its lab app and waiting results, in one sentence.
    pub(super) fn kept_sentence(&self) -> String {
        match &self.kept {
            Ok(Some(destination)) => format!(
                concat!(
                    "Lab app {} stays on the Lab apps page; results still waiting for it are sent ",
                    "after the restart until none are left"
                ),
                destination
            ),
            Ok(None) => "It was not set up, so it had no lab app".into(),
            Err(error) => {
                format!(
                    "Its lab app settings could not be read, so waiting results stay stored until that lab app is added on the Lab apps page: {error}"
                )
            }
        }
    }

    /// The audit reason for removing machine `id`.
    pub(super) fn reason(&self, via: Via, id: &str) -> String {
        format!(
            "{} removed machine {id}; store untouched; at removal: {}; removed anyway: {}; {}; {}",
            via.machines(),
            self.summary,
            if self.anyway { "yes" } else { "no" },
            self.site_sentence(),
            self.kept_sentence()
        )
    }
}

/// Refuses while something is unfinished unless `anyway`, then takes machine
/// `id` out of the program file; the store is untouched.
pub(super) async fn remove_machine(
    setup: &Arc<Setup>,
    store: &analyte_store::Handle,
    id: &str,
    anyway: bool,
) -> Result<Removed, SetupError> {
    let open = unfinished(store, id).await;
    let summary = match &open {
        Ok(open) if open.is_empty() => "nothing unfinished".to_owned(),
        Ok(open) => open_work(open).join("; "),
        Err(error) => format!("unfinished work could not be counted: {error}"),
    };
    if !anyway && !matches!(&open, Ok(open) if open.is_empty()) {
        return Err(SetupError::UnfinishedWork(id.to_owned(), summary));
    }
    let machine = id.to_owned();
    let mut removed = off_thread(setup, move |setup| remove_blocking(setup, &machine)).await?;
    removed.summary = summary;
    removed.anyway = anyway;
    Ok(removed)
}

fn remove_blocking(setup: &Setup, id: &str) -> Result<Removed, SetupError> {
    let _writing = setup.writing()?;
    let config = setup.config();
    let (program, listed) = site::entries(config)?;
    let (_, installed) = site::installed(config, id)?;
    let kept = configured_destination(config, id);
    let mut changes = vec![(config.to_owned(), entries::removed(&program, &listed, id)?)];
    // A lab app the site file still wrote moves into lab-apps.toml, so results
    // waiting for it stay deliverable after the site file is moved away.
    if let Ok(Some(destination)) = &kept {
        let lab_apps = crate::installation::config::lab_apps::read(config)?;
        let after = site::moved(&lab_apps, destination.clone())?;
        if after != lab_apps {
            changes.push((
                crate::installation::config::lab_apps::path(config),
                after.text()?,
            ));
        }
    }
    let written = site::commit(config, &changes)?;
    let folder = folder(config);
    let shared = listed.iter().any(|entry| {
        entry.id.get_ref() != id && same_path(&folder.join(entry.site.get_ref()), &installed.site)
    });
    let site = if shared {
        Ok(None)
    } else {
        site::retire(&installed.site)
    };
    Ok(Removed {
        backups: written.backups,
        kept: kept.map(|kept| kept.map(|destination| destination.name.as_str().to_owned())),
        site,
        summary: String::new(),
        anyway: false,
    })
}

/// POST /setup/{id}/remove: take the entry out of the program file; the store is untouched.
pub(in crate::admin) async fn remove(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: Result<Form<Removal>, axum::extract::rejection::FormRejection>,
) -> Response {
    let refused = |error: &SetupError| {
        Problem::Refused(error.to_string()).page(
            &state,
            Some(&session),
            Section::Machines(Some(&id)),
        )
    };
    let Ok(Form(removal)) = form else {
        return refused(&SetupError::IncompleteForm(RELOAD));
    };
    let anyway = removal.anyway.as_deref() == Some("yes");
    let removed = match remove_machine(&state.setup, &state.store, &id, anyway).await {
        Ok(removed) => removed,
        Err(error) => return refused(&error),
    };
    let kept = removed.kept_sentence();
    let reason = removed.reason(Via::Page, &id);
    let audited = audit(&state, &session, "setup-remove", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page removed a machine");
    let mut lines = Vec::new();
    let mut problems = Vec::new();
    let mut support = copies(&removed.backups);
    match &removed.site {
        Ok(Some(path)) => support.push(format!(
            "Its settings file, kept as a dated copy: {}",
            path.display()
        )),
        Ok(None) => lines.push(
            "Its settings file was left in place (missing or used by another machine).".into(),
        ),
        Err(error) => problems.push(format::sentence(&format!(
            "its settings file could not be moved: {error}"
        ))),
    }
    if !support.is_empty() {
        lines.insert(0, crate::admin::setup::BACKUP_KEPT.into());
    }
    if let Err(error) = audited {
        problems.push(format::sentence(&error.to_string()));
    }
    machines::removed(
        &state,
        &session,
        &id,
        machines::Removed {
            kept: format!("{kept}."),
            lines,
            problems,
            support,
            csrf: session.csrf.form_value().to_owned(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decisions 2026-11-20 (admin restyle b2) and AGENTS.md safety rule
    // 2: the Remove dialog lists open work in the setup page's own words and
    // asks for "Remove anyway"; work that could not be counted asks for it too
    // and says where to look.
    #[test]
    fn open_work_or_why_it_was_not_counted_asks_for_remove_anyway() {
        let (items, warned) = warning(&Ok(Unfinished::default()));
        assert!(!warned);
        assert_eq!(
            items.first().map(|item| (item.tone, item.text.as_str())),
            Some((Tone::Good, "Nothing is unfinished for this machine."))
        );
        let (items, warned) = warning(&Ok(Unfinished {
            undelivered: 2,
            open_orders: 0,
            unread: 1,
            flagged: 0,
        }));
        assert!(warned);
        let texts: Vec<&str> = items.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "2 results or order messages not yet accepted by the lab app",
                "1 received message not read yet; only this machine's settings read them, at its next start",
                "Removing does not delete any of this. Finish it first, or remove anyway. Items to review that are not linked to one machine are not counted.",
            ]
        );
        let (items, warned) = warning(&Err(SetupError::Busy));
        assert!(warned);
        assert_eq!(items.len(), 1);
        assert_eq!(items.first().map(|item| item.tone), Some(Tone::Problem));
        assert!(items.first().is_some_and(|item| item.text.starts_with(
            "Unfinished work could not be counted: an earlier setup change failed while writing"
        )));
    }

    // Owner request 2026-11-19 (item 9) and owner decisions 2026-11-20: what
    // happens to waiting results is one sentence, with the exact count on hover
    // only when some wait.
    #[test]
    fn waiting_results_read_as_one_sentence() {
        let none = delivery_sentence(&LabApp::None, &Ok(Unfinished::default()));
        assert_eq!(
            none.shown,
            "It is not set up, so nothing is waiting to be sent."
        );
        assert!(none.exact.is_empty());
        let kept = LabApp::Kept("demo".into());
        let waiting = delivery_sentence(
            &kept,
            &Ok(Unfinished {
                undelivered: 1,
                ..Unfinished::default()
            }),
        );
        assert_eq!(
            waiting.shown,
            "Its 1 waiting result will still be sent to demo."
        );
        assert_eq!(
            waiting.exact,
            "1 result or order message not yet accepted by the lab app"
        );
        let idle = delivery_sentence(&kept, &Ok(Unfinished::default()));
        assert!(
            idle.shown.starts_with("Nothing is waiting;"),
            "{}",
            idle.shown
        );
        let uncounted = delivery_sentence(&kept, &Err(SetupError::Busy));
        assert_eq!(
            uncounted.shown,
            "Its waiting results will still be sent to demo."
        );
    }
}
