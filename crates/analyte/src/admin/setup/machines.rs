//! Adds and removes machines on the setup page: the forms, the checks, the warnings and the audit.
//!
//! Both write through the same checked commit as Save (backup, safe write, reload,
//! rollback) and apply after a restart. Removing never touches the store: a
//! removed machine's receipts, flagged items and history stay listed for staff.
//! Its lab app stays in `lab-apps.toml` (moved there first if the site file
//! still wrote it), so results waiting for it are still delivered. A machine just
//! added goes straight into step 1 of the setup steps.
use super::{SetupError, after_write, audit, done, entries, needs_login, page, refused, site};
use crate::admin::{markup::Markup, server::Context, sessions::Session, view::plain};
use crate::installation::config::Program;
use analyte_model::Identifier;
use analyte_store::Unfinished;
use axum::{
    Extension, Form,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse as _, Response},
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

/// GET /setup/add: the form for one more machine, with the next free name filled in.
pub(in crate::admin) async fn add_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let worker = state.clone();
    let suggested = tokio::task::spawn_blocking(move || {
        site::entries(&worker.config_path).map(|(_, listed)| entries::suggestion(&listed))
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    let suggested = match suggested {
        Ok(name) => name,
        Err(error) => return refused(&error),
    };
    done(
        page("Add a machine")
            .markup(concat!(
                "<p class=\"lead\">Adds a machine that is not set up yet. Analyte checks its ",
                "settings, keeps a copy of the old ones and writes the new ones; if anything ",
                "fails, the old settings are put back. Then the setup steps follow: its model, ",
                "its connection, a test and the save.</p>",
                "<section><form method=\"post\" action=\"/setup/add\"><input type=\"hidden\" ",
                "name=\"csrf\" value=\""
            ))
            .text(session.csrf.form_value())
            .markup("\"><label class=\"field\"><span>Machine name</span><input name=\"name\" required maxlength=\"64\" value=\"")
            .text(&suggested)
            .markup(concat!(
                "\"></label><p class=\"muted\">Use letters, digits, - and _. The name also names the ",
                "machine's settings file (<code>&lt;name&gt;.site.toml</code>).</p>",
                "<div class=\"notice\"><p><strong>Reusing a name.</strong> Analyte keeps results, items to review and ",
                "history by machine name. A machine added under the name of one removed ",
                "earlier joins that stored history. Use an earlier name only for the same ",
                "analyzer; give a different analyzer a new name, so its results are never ",
                "matched with the old one's. If the name has stored history, tick this box ",
                "to confirm:</p><label class=\"check\"><input type=\"checkbox\" name=\"same_analyzer\" ",
                "value=\"yes\"> This is the same analyzer that used this name before</label></div>",
                "<div class=\"actions-row\"><button>Add machine</button>",
                "<a class=\"button secondary\" href=\"/setup\">Cancel</a></div></form></section>"
            )),
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

fn add_blocking(state: &Context, id: &Identifier) -> Result<(PathBuf, site::Written), SetupError> {
    let _writing = state.setup.writing()?;
    let config = &state.config_path;
    let (program, listed) = site::entries(config)?;
    if entries::taken(&listed, id.as_str()) {
        return Err(SetupError::NameInUse(id.as_str().into()));
    }
    let file = entries::site_file(id);
    let folder = site::folder(config);
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
            (path.clone(), entries::starter_site(id)),
            (config.clone(), edited),
        ],
    )?;
    Ok((path, written))
}

/// POST /setup/add: check the name, add the entry and its starter site file.
pub(in crate::admin) async fn add(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    form: Result<Form<Addition>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(addition)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; reload the page",
        ));
    };
    let id = match entries::name(&addition.name) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    // A listed name is refused as such before its history is looked at;
    // `add_blocking` checks again under the write lock.
    let worker = state.clone();
    let name = id.clone();
    let listed = tokio::task::spawn_blocking(move || {
        site::entries(&worker.config_path).map(|(_, listed)| entries::taken(&listed, name.as_str()))
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    match listed {
        Ok(false) => {}
        Ok(true) => return refused(&SetupError::NameInUse(id.as_str().into())),
        Err(error) => return refused(&error),
    }
    let reused = match history(&state, &id).await {
        Ok(reused) => reused,
        Err(error) => return refused(&error),
    };
    if reused && addition.same_analyzer.as_deref() != Some("yes") {
        return refused(&SetupError::HistoryNotConfirmed(id.as_str().into()));
    }
    let worker = state.clone();
    let machine = id.clone();
    let added = tokio::task::spawn_blocking(move || add_blocking(&worker, &machine))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r);
    let (path, written) = match added {
        Ok(added) => added,
        Err(error) => return refused(&error),
    };
    let reason = format!(
        "setup page added machine {} with starter site file {}{}",
        id.as_str(),
        path.display(),
        if reused {
            "; joins that name's stored history (same analyzer confirmed)"
        } else {
            ""
        }
    );
    let audited = audit(&state, &session, "setup-add", id.as_str(), reason).await;
    tracing::info!(machine = id.as_str(), "setup page added a machine");
    // Straight into step 1, unless the audit failed: that is shown, never hidden.
    if audited.is_ok() {
        return see_other(&format!("/setup/{}", id.as_str()));
    }
    let mut html = page("Machine added")
        .markup("<p class=\"notice info\">Machine <strong>")
        .text(id.as_str())
        .markup("</strong> is listed and <strong>Not set up yet</strong>. <span class=\"muted\">Its settings file is <code>")
        .text(path.display())
        .markup("</code>.</span> <a href=\"/setup/")
        .text(id.as_str())
        .markup("\">Set it up now</a>, or restart first.</p>");
    if reused {
        html.push_markup(
            "<p>It joins the stored history of the earlier machine with this name.</p>",
        );
    }
    html.push_fragment(&backups_kept(&written.backups));
    after_write(html, audited, &session)
}

/// See Other to `location`; a machine name that cannot be a header value (only
/// possible in a hand-written program file) goes to the setup page instead.
fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let location = header::HeaderValue::from_str(location)
        .unwrap_or_else(|_| header::HeaderValue::from_static("/setup"));
    response.headers_mut().insert(header::LOCATION, location);
    response
}

/// GET /setup/first: the first-run button. Step 1 for the first listed machine
/// that is not set up yet; Add machine when none is listed; otherwise the setup page.
pub(in crate::admin) async fn first(State(state): State<Arc<Context>>) -> Response {
    let worker = state.clone();
    let listed = tokio::task::spawn_blocking(move || {
        site::entries(&worker.config_path).map(|(_, listed)| {
            let waiting = listed
                .iter()
                .find(|entry| entry.definition.is_none())
                .map(|entry| entry.id.get_ref().clone());
            (waiting, listed.is_empty())
        })
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    match listed {
        Ok((Some(id), _)) => see_other(&format!("/setup/{id}")),
        Ok((None, true)) => see_other("/setup/add"),
        Ok((None, false)) => see_other("/setup"),
        Err(error) => refused(&error),
    }
}

fn backups_kept(backups: &[PathBuf]) -> Markup {
    let mut html = Markup::new().markup("<ul class=\"muted\">");
    for backup in backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Copy of the old settings: ")
                .text(backup.display())
                .markup("</li>"),
        );
    }
    html.markup("</ul>")
}

async fn history(state: &Context, id: &Identifier) -> Result<bool, SetupError> {
    let machine = id.clone();
    state
        .store
        .request::<_, analyte_store::OperatorError>(
            crate::deadline().map_err(|e| SetupError::Store(e.to_string()))?,
            move |store| store.has_history(&machine),
        )
        .await
        .map_err(|e| SetupError::Store(e.to_string()))
}

async fn unfinished(state: &Context, id: &str) -> Result<Unfinished, SetupError> {
    let machine = Identifier::new(id).map_err(SetupError::InvalidName)?;
    state
        .store
        .request::<_, analyte_store::OperatorError>(
            crate::deadline().map_err(|e| SetupError::Store(e.to_string()))?,
            move |store| store.unfinished(&machine),
        )
        .await
        .map_err(|e| SetupError::Store(e.to_string()))
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
    .map(|(count, one, many, rest)| format!("{}{rest}", plain::amount(count, one, many)))
    .collect()
}

/// The lab app machine `id` is configured with (overrides included), which
/// removing keeps in `lab-apps.toml`; none while it waits for setup. An error
/// says why it cannot be read, and then nothing is kept.
fn configured_destination(
    config: &std::path::Path,
    id: &str,
) -> Result<Option<analyte_config::Destination>, String> {
    let program = Program::load(config).map_err(|error| error.to_string())?;
    Ok(program
        .machines()
        .iter()
        .find(|machine| machine.id().as_str() == id)
        .map(|machine| machine.snapshot().effective().site().destination.clone()))
}

/// The lab app a machine sends to, as far as removing it is concerned.
enum LabApp {
    /// Not set up: it has no lab app.
    None,
    /// Its settings cannot be read, so its lab app cannot be kept.
    Unreadable(String),
    /// The lab app stays on the Lab app page (moved into `lab-apps.toml` first if
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

/// One sentence on what happens to the machine's waiting results after removal.
fn delivery_sentence(lab_app: &LabApp, open: &Result<Unfinished, SetupError>) -> Markup {
    match lab_app {
        LabApp::None => Markup::new().text("It is not set up, so nothing is waiting to be sent."),
        LabApp::Unreadable(error) => Markup::new().text(format!(
            concat!(
                "Its lab app settings cannot be read ({}), so results still waiting for it are ",
                "kept, never deleted, until that lab app is added on the Lab app page."
            ),
            error
        )),
        LabApp::Kept(name) => match open {
            Ok(open) if open.undelivered > 0 => Markup::new()
                .markup("<span title=\"")
                .text(plain::amount(
                    open.undelivered,
                    "result or order message not yet accepted by the lab app",
                    "results or order messages not yet accepted by the lab app",
                ))
                .markup("\">")
                .text(format!(
                    "Its {} will still be sent to {name}.",
                    plain::amount(open.undelivered, "waiting result", "waiting results")
                ))
                .markup("</span>"),
            Ok(_) => Markup::new().text(format!(
                "Nothing is waiting; anything it sends before the restart will still be sent to {name}."
            )),
            Err(_) => Markup::new().text(format!(
                "Its waiting results will still be sent to {name}."
            )),
        },
    }
}

/// GET /setup/{id}/remove: the plain confirmation, with any unfinished work.
pub(in crate::admin) async fn confirm_remove(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(id): Path<String>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let worker = state.clone();
    let machine = id.clone();
    let facts = tokio::task::spawn_blocking(move || {
        site::installed(&worker.config_path, &machine)?;
        let kept = lab_app(&worker.config_path, &machine);
        let (_, listed) = site::entries(&worker.config_path)?;
        Ok::<_, SetupError>((kept, listed.len() == 1))
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    let (kept, last) = match facts {
        Ok(facts) => facts,
        Err(error) => return refused(&error),
    };
    let open = unfinished(&state, &id).await;
    let mut html = page("Remove a machine")
        .markup("<p class=\"lead\"><strong>Remove ")
        .text(&id)
        .markup(concat!(
            "?</strong> Its results, items to review and history are kept and stay visible ",
            "here; Analyte just stops connecting to it.</p>"
        ));
    let warned = warning(&mut html, &open);
    html.push_fragment(
        &Markup::new()
            .markup("<section><h2>Waiting results</h2><p class=\"removal\">")
            .fragment(&delivery_sentence(&kept, &open))
            .markup("</p>"),
    );
    if last {
        html.push_markup(concat!(
            "<p class=\"notice\">This is the last machine. After the restart Analyte serves only this ",
            "admin page until a machine is added.</p>"
        ));
    }
    html.push_fragment(
        &Markup::new()
            .markup(concat!(
                "<p class=\"muted\">Analyte keeps dated copies of the old settings. Removing ",
                "applies after a restart; until then the machine keeps running.</p></section>",
                "<form method=\"post\" action=\""
            ))
            .text(format!("/setup/{id}/remove"))
            .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
            .text(session.csrf.form_value())
            .markup("\">"),
    );
    html.push_markup(if warned {
        concat!(
            "<input type=\"hidden\" name=\"anyway\" value=\"yes\"><div class=\"actions-row\">",
            "<button class=\"danger\">Remove anyway</button>"
        )
    } else {
        "<div class=\"actions-row\"><button class=\"danger\">Remove</button>"
    });
    html.push_markup("<a class=\"button secondary\" href=\"/setup\">Keep it</a></div></form>");
    done(html)
}

/// Shows open work or why it could not be counted; true when there is a warning.
fn warning(html: &mut Markup, open: &Result<Unfinished, SetupError>) -> bool {
    match open {
        Ok(open) if open.is_empty() => {
            html.push_markup(concat!(
                "<p class=\"notice info\">Nothing is unfinished for this machine. (Items to review ",
                "that are not linked to one machine are not counted.)</p>"
            ));
            false
        }
        Ok(open) => {
            html.push_markup(
                "<div class=\"notice\"><p><strong>Unfinished for this machine:</strong></p><ul>",
            );
            for line in open_work(open) {
                html.push_fragment(&Markup::new().markup("<li>").text(line).markup("</li>"));
            }
            html.push_markup(concat!(
                "</ul><p>Removing does not delete any of this. Finish it first, or ",
                "remove anyway. (Items to review that are not linked to one machine are not ",
                "counted.)</p></div>"
            ));
            true
        }
        Err(error) => {
            html.push_fragment(
                &Markup::new()
                    .markup("<p class=\"notice bad\"><strong>Unfinished work could not be counted:</strong> ")
                    .text(error)
                    .markup(
                        ". Check the Deliveries, Machine orders and Needs review pages first.</p>",
                    ),
            );
            true
        }
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

struct Removed {
    backups: Vec<PathBuf>,
    /// The lab app kept for delivery, none for a machine not set up, or why its
    /// settings could not be read (then nothing is kept).
    kept: Result<Option<String>, String>,
    /// Where the site file went: a dated copy, nowhere (missing or still named by
    /// another machine), or why moving it failed after the program was changed.
    site: Result<Option<PathBuf>, String>,
}

fn remove_blocking(state: &Context, id: &str) -> Result<Removed, SetupError> {
    let _writing = state.setup.writing()?;
    let config = &state.config_path;
    let (program, listed) = site::entries(config)?;
    let (_, installed) = site::installed(config, id)?;
    let kept = configured_destination(config, id);
    let mut changes = vec![(config.clone(), entries::removed(&program, &listed, id)?)];
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
    let folder = site::folder(config);
    let shared = listed.iter().any(|entry| {
        entry.id.get_ref() != id && same_path(&folder.join(entry.site.get_ref()), &installed.site)
    });
    let site = if shared {
        Ok(None)
    } else {
        site::retire(&installed.site).map_err(|error| error.to_string())
    };
    Ok(Removed {
        backups: written.backups,
        kept: kept.map(|kept| kept.map(|destination| destination.name.as_str().to_owned())),
        site,
    })
}

/// POST /setup/{id}/remove: take the entry out of the program file; the store is untouched.
pub(in crate::admin) async fn remove(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    form: Result<Form<Removal>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(removal)) = form else {
        return refused(&SetupError::Connection(
            "the form was incomplete; reload the page",
        ));
    };
    let anyway = removal.anyway.as_deref() == Some("yes");
    let open = unfinished(&state, &id).await;
    let summary = match &open {
        Ok(open) if open.is_empty() => "nothing unfinished".to_owned(),
        Ok(open) => open_work(open).join("; "),
        Err(error) => format!("unfinished work could not be counted: {error}"),
    };
    if !anyway && !matches!(&open, Ok(open) if open.is_empty()) {
        return refused(&SetupError::UnfinishedWork(id, summary));
    }
    let worker = state.clone();
    let machine = id.clone();
    let removed = tokio::task::spawn_blocking(move || remove_blocking(&worker, &machine))
        .await
        .map_err(SetupError::from)
        .and_then(|r| r);
    let removed = match removed {
        Ok(removed) => removed,
        Err(error) => return refused(&error),
    };
    let site = match &removed.site {
        Ok(Some(path)) => format!(
            "Its settings file was kept as a dated copy: {}",
            path.display()
        ),
        Ok(None) => {
            "Its settings file was left in place (missing or used by another machine)".into()
        }
        Err(error) => format!("Its settings file could not be moved: {error}"),
    };
    let kept = match &removed.kept {
        Ok(Some(destination)) => format!(
            concat!(
                "Lab app {} stays on the Lab app page; results still waiting for it are sent ",
                "after the restart until none are left"
            ),
            destination
        ),
        Ok(None) => "It was not set up, so it had no lab app".into(),
        Err(error) => {
            format!(
                "Its lab app settings could not be read, so waiting results stay stored until that lab app is added on the Lab app page: {error}"
            )
        }
    };
    let reason = format!(
        "setup page removed machine {id}; store untouched; at removal: {summary}; removed anyway: {}; {site}; {kept}",
        if anyway { "yes" } else { "no" }
    );
    let audited = audit(&state, &session, "setup-remove", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page removed a machine");
    let mut html = page("Machine removed")
        .markup("<section><p>Machine <strong>")
        .text(&id)
        .markup(concat!(
            "</strong> is removed from Analyte's settings. Its results, items to review and ",
            "history are kept and stay visible here.</p><p>"
        ))
        .text(kept)
        .markup(".</p><p class=\"muted\">")
        .text(site)
        .markup(".</p>");
    html.push_fragment(&backups_kept(&removed.backups).markup("</section>"));
    after_write(html, audited, &session)
}
