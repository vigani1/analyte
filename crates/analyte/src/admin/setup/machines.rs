//! Adds and removes machines on the setup page: the forms, the checks, the warnings and the audit.
//!
//! Both write through the same checked commit as Save (backup, safe write, reload,
//! rollback) and apply after a restart. Removing never touches the store: a
//! removed machine's receipts, flagged items and history stay listed for staff.
use super::{SetupError, audit, done, entries, needs_login, page, refused, restart_form, site};
use crate::admin::{markup::Markup, server::Context, sessions::Session};
use crate::installation::config::Program;
use analyte_model::Identifier;
use analyte_store::Unfinished;
use axum::{
    Extension, Form,
    extract::{Path, State},
    response::Response,
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
                "<p>Adds one machine that is not set up yet, with a starter site file that ",
                "holds only the retention and the key names. Analyte checks the program file, ",
                "keeps a copy of it, writes it and loads it again; if that fails, the previous ",
                "file is put back. The new machine shows as <strong>Not set up yet</strong>: ",
                "set it up like any other, then restart.</p>",
                "<form method=\"post\" action=\"/setup/add\"><input type=\"hidden\" ",
                "name=\"csrf\" value=\""
            ))
            .text(session.csrf.form_value())
            .markup("\"><label>Machine name <input name=\"name\" required maxlength=\"64\" value=\"")
            .text(&suggested)
            .markup(concat!(
                "\"></label><p>Use letters, digits, - and _. The name also names the ",
                "machine's site file (<code>&lt;name&gt;.site.toml</code> beside the program file).</p>",
                "<p><strong>Reusing a name.</strong> Analyte keeps results, flagged items and ",
                "history by machine name. A machine added under the name of one removed ",
                "earlier joins that stored history. Use an earlier name only for the same ",
                "analyzer; give a different analyzer a new name, so its results are never ",
                "matched with the old one's. If the name has stored history, tick this box ",
                "to confirm:</p><label><input type=\"checkbox\" name=\"same_analyzer\" ",
                "value=\"yes\"> This is the same analyzer that used this name before</label> ",
                "<button>Add machine</button></form></html>"
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
    let mut html = page("Machine added")
        .markup("<p>Machine <strong>")
        .text(id.as_str())
        .markup("</strong> is listed and <strong>Not set up yet</strong>. Its starter site file is <code>")
        .text(path.display())
        .markup("</code>. <a href=\"/setup/")
        .text(id.as_str())
        .markup("\">Set it up now</a>, or restart first.</p>");
    if reused {
        html.push_markup(
            "<p>It joins the stored history of the earlier machine with this name.</p>",
        );
    }
    html.push_fragment(&kept(&written.backups));
    if let Err(error) = audited {
        html.push_fragment(&Markup::new().markup("<p>").text(error).markup("</p>"));
    }
    html.push_fragment(&restart_form(&session));
    done(html)
}

fn kept(backups: &[PathBuf]) -> Markup {
    let mut html = Markup::new().markup("<ul>");
    for backup in backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Previous file kept as ")
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
            "results or order messages not yet accepted by the lab app",
        ),
        (
            open.open_orders,
            concat!(
                "orders sent to the machine and not finished (sending, sent, accepted, ",
                "cancel requested or uncertain); once it is removed nothing serves them, so ",
                "reconcile them with the machine first (Machine orders page)"
            ),
        ),
        (
            open.unread,
            "received messages not read yet; only this machine's settings read them, at its next start",
        ),
        (
            open.flagged,
            "unresolved flagged items linked to this machine (Parked items page)",
        ),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, what)| format!("{count} {what}"))
    .collect()
}

/// What happens to delivery after removal: workers run per lab app destination,
/// built from the machines that remain (`runtime::delivery::destinations`).
fn delivery_note(state: &Context, id: &str) -> Result<String, SetupError> {
    let (_, installed) = site::installed(&state.config_path, id)?;
    let destination = crate::installation::config::read(&installed.site)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| {
            table
                .get("destination")?
                .get("name")?
                .as_str()
                .map(str::to_owned)
        });
    let Some(destination) = destination else {
        return Ok("This machine has no lab app destination yet, so no stored result waits to be delivered for it.".into());
    };
    Ok(match Program::load(&state.config_path) {
        Ok(program) => {
            let others: Vec<&str> = program
                .machines()
                .iter()
                .filter(|machine| {
                    machine.id().as_str() != id
                        && machine
                            .snapshot()
                            .effective()
                            .site()
                            .destination
                            .name
                            .as_str()
                            == destination
                })
                .map(|machine| machine.id().as_str())
                .collect();
            if others.is_empty() {
                format!(
                    concat!(
                        "No other set-up machine uses lab app destination {}. Delivery runs ",
                        "per destination, so after the restart nothing delivers to it until a ",
                        "machine using it is set up again; undelivered results stay stored and ",
                        "are never deleted."
                    ),
                    destination
                )
            } else {
                format!(
                    concat!(
                        "Results already stored keep being delivered after the restart: delivery ",
                        "runs per lab app destination, not per machine, and {} still use ",
                        "destination {}."
                    ),
                    others.join(", "),
                    destination
                )
            }
        }
        Err(error) => format!(
            "Whether another machine still delivers to destination {destination} cannot be checked now: {error}"
        ),
    })
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
        let note = delivery_note(&worker, &machine)?;
        let (_, listed) = site::entries(&worker.config_path)?;
        Ok::<_, SetupError>((note, listed.len() == 1))
    })
    .await
    .map_err(SetupError::from)
    .and_then(|r| r);
    let (note, last) = match facts {
        Ok(facts) => facts,
        Err(error) => return refused(&error),
    };
    let open = unfinished(&state, &id).await;
    let mut html = page("Remove a machine")
        .markup("<p><strong>Remove ")
        .text(&id)
        .markup(concat!(
            "?</strong> Its stored results, flagged items and history stay in the ",
            "database and remain visible to staff; the machine just stops being served.</p>"
        ));
    let warned = warning(&mut html, &open);
    html.push_fragment(&Markup::new().markup("<p>").text(note).markup("</p>"));
    if last {
        html.push_markup(concat!(
            "<p>This is the last machine. After the restart Analyte serves only this ",
            "admin page until a machine is added.</p>"
        ));
    }
    html.push_fragment(
        &Markup::new()
            .markup(concat!(
                "<p>The program file keeps a dated copy, and the machine's site file is ",
                "moved to a dated copy, not deleted. Removing applies after a restart; ",
                "until then the machine keeps running.</p><form method=\"post\" action=\""
            ))
            .text(format!("/setup/{id}/remove"))
            .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
            .text(session.csrf.form_value())
            .markup("\">"),
    );
    html.push_markup(if warned {
        "<input type=\"hidden\" name=\"anyway\" value=\"yes\"><button>Remove anyway</button>"
    } else {
        "<button>Remove</button>"
    });
    html.push_markup(" <a href=\"/setup\">Keep it</a></form></html>");
    done(html)
}

/// Shows open work or why it could not be counted; true when there is a warning.
fn warning(html: &mut Markup, open: &Result<Unfinished, SetupError>) -> bool {
    match open {
        Ok(open) if open.is_empty() => {
            html.push_markup(concat!(
                "<p>Nothing is unfinished for this machine. (Flagged items not linked to ",
                "one machine are not counted.)</p>"
            ));
            false
        }
        Ok(open) => {
            html.push_markup("<p><strong>Unfinished for this machine:</strong></p><ul>");
            for line in open_work(open) {
                html.push_fragment(&Markup::new().markup("<li>").text(line).markup("</li>"));
            }
            html.push_markup(concat!(
                "</ul><p>Removing does not delete any of this. Finish it first, or ",
                "remove anyway. (Flagged items not linked to one machine are not counted.)</p>"
            ));
            true
        }
        Err(error) => {
            html.push_fragment(
                &Markup::new()
                    .markup("<p><strong>Unfinished work could not be counted:</strong> ")
                    .text(error)
                    .markup(
                        ". Check the Deliveries, Machine orders and Parked items pages first.</p>",
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
    /// Where the site file went: a dated copy, nowhere (missing or still named by
    /// another machine), or why moving it failed after the program was changed.
    site: Result<Option<PathBuf>, String>,
}

fn remove_blocking(state: &Context, id: &str) -> Result<Removed, SetupError> {
    let _writing = state.setup.writing()?;
    let config = &state.config_path;
    let (program, listed) = site::entries(config)?;
    let (_, installed) = site::installed(config, id)?;
    let edited = entries::removed(&program, &listed, id)?;
    let written = site::commit(config, &[(config.clone(), edited)])?;
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
        Ok(Some(path)) => format!("site file kept as {}", path.display()),
        Ok(None) => "site file left in place (missing or used by another machine)".into(),
        Err(error) => format!("site file could not be moved: {error}"),
    };
    let reason = format!(
        "setup page removed machine {id}; store untouched; at removal: {summary}; removed anyway: {}; {site}",
        if anyway { "yes" } else { "no" }
    );
    let audited = audit(&state, &session, "setup-remove", &id, reason).await;
    tracing::info!(machine = id.as_str(), "setup page removed a machine");
    let mut html = page("Machine removed")
        .markup("<p>Machine <strong>")
        .text(&id)
        .markup(concat!(
            "</strong> is no longer listed in the program file. Its stored results, ",
            "flagged items and history stay in the database and remain visible to staff.</p><p>"
        ))
        .text(site)
        .markup(".</p>");
    html.push_fragment(&kept(&removed.backups));
    if let Err(error) = audited {
        html.push_fragment(&Markup::new().markup("<p>").text(error).markup("</p>"));
    }
    html.push_fragment(&restart_form(&session));
    done(html)
}
