//! Serves Settings (GET /settings): what Analyte runs with, read-only and only real values from the loaded program and this installation; where each key is found, never its value.
use super::page::{KeyRow, Settings};
use crate::admin::server::Context;
use crate::admin::server::sessions::Session;
use crate::admin::view::{
    self, Tab, Tone, format,
    frame::Frame,
    report::{Fact, Problem, Section, Value, read_within, respond},
};
use crate::installation::config::Program;
use crate::machine::link::status::Light;
use analyte_i18n::{count as tr_count, message as tr_args, text as tr};
use analyte_model::Identifier;
use askama::Template as _;
use axum::{Extension, extract::State, http::StatusCode, response::Response};
use jiff::Timestamp;
use std::{sync::Arc, time::Duration};

/// What the loaded program set that Settings shows, kept when the admin
/// server starts; nothing here is a key's value or an address that may hold one.
pub(in crate::admin) struct Installed {
    database_bytes: u64,
    record_bytes: usize,
    busy_timeout: Duration,
    startup_timeout: Duration,
    /// How long the service may take to stop.
    shutdown: Duration,
    alert_interval: Duration,
    alert_reminder: Duration,
    alert_timeout: Duration,
    alert_attempts: u32,
    /// Each alert target and the name of its signing key; never its URL.
    alert_targets: Vec<(Identifier, Identifier)>,
}

impl Installed {
    /// What `program` set, with the service's shutdown limit.
    pub(in crate::admin) fn of(program: &Program, shutdown: Duration) -> Self {
        let delivery = program.alert_delivery();
        Self {
            database_bytes: program.store_limits.database_bytes,
            record_bytes: program.store_limits.record_bytes,
            busy_timeout: program.store_limits.busy_timeout,
            startup_timeout: program.store_startup_timeout(),
            shutdown,
            alert_interval: program.alert_interval(),
            alert_reminder: delivery.reminder(),
            alert_timeout: delivery.timeout(),
            alert_attempts: delivery.attempts(),
            alert_targets: delivery
                .endpoints()
                .iter()
                .map(|endpoint| (endpoint.id().clone(), endpoint.secret().clone()))
                .collect(),
        }
    }

    /// Plain values for tests that build a server without a program.
    #[cfg(all(test, feature = "test-hooks"))]
    pub(in crate::admin) fn for_test(alert_targets: Vec<(Identifier, Identifier)>) -> Self {
        Self {
            database_bytes: 1_073_741_824,
            record_bytes: 1_048_576,
            busy_timeout: Duration::from_secs(5),
            startup_timeout: Duration::from_secs(30),
            shutdown: Duration::from_secs(30),
            alert_interval: Duration::from_secs(30),
            alert_reminder: Duration::from_secs(3600),
            alert_timeout: Duration::from_secs(10),
            alert_attempts: 5,
            alert_targets,
        }
    }
}

pub(in crate::admin) async fn settings(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let section = Section::More(Tab::Settings);
    let now = Timestamp::now();
    let targets = state.installed.alert_targets.clone();
    let sources = crate::admin::setup::key_sources(&state, targets, now);
    let read = read_within(async {
        sources
            .await
            .map_err(|error| Problem::Settings(error.to_string()))
    });
    let (found, unlisted, unreadable) = match read.await {
        Ok(keys) => keys,
        Err(problem) => return problem.page(&state, session, section),
    };
    let mut problems = Vec::new();
    let mut support = Vec::new();
    if let Some(why) = unlisted {
        problems.push(tr("admin-the-lab-apps-settings-could-not-be-read-so").to_owned());
        support.push(tr_args(
            "admin-lab-apps",
            [("why", (why).to_string().into())],
        ));
    }
    if let Some(why) = unreadable {
        support.push(why);
    }
    let page = Settings {
        languages: if analyte_i18n::Language::installed().count() > 1 {
            analyte_i18n::Language::installed()
                .map(|language| {
                    (
                        language.tag(),
                        language == analyte_i18n::Language::current(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        },
        frame: Frame::new(
            &state,
            Tab::Settings,
            Tab::Settings.label(),
            None,
            session,
            now,
        ),
        engine: engine(&state),
        store: store(&state),
        times: times(&state),
        alerts: alerts(&state),
        keys: keys(&state, found),
        problems,
        support,
    };
    respond(&state, session, section, StatusCode::OK, page.render())
}

/// A path in mono, in full.
fn path(path: &std::path::Path) -> Value {
    let shown = path.display().to_string();
    Value::Code {
        exact: shown.clone(),
        shown,
    }
}

/// A size such as "1 GB", with the exact bytes on hover.
fn size(bytes: u64) -> Value {
    Value::Code {
        shown: format::bytes(bytes),
        exact: tr_count("admin-count-bytes", bytes),
    }
}

/// A time limit such as "1.5 s", with the exact milliseconds on hover.
fn limit(limit: Duration) -> Value {
    Value::Code {
        shown: format::limit(limit),
        exact: format!("{} ms", limit.as_millis()),
    }
}

/// Analyte itself, which answered this page: its version, how many machine
/// links are open now, where its pages, program file and data are, and the
/// time zone its pages use.
fn engine(state: &Context) -> Vec<Fact> {
    let open = state
        .machines
        .iter()
        .filter(|machine| machine.status.borrow().light() == Light::Live)
        .count();
    let mut facts = vec![
        Fact {
            term: tr("admin-analyte-version"),
            value: Value::Code {
                shown: env!("CARGO_PKG_VERSION").into(),
                exact: env!("CARGO_PKG_VERSION").into(),
            },
        },
        Fact {
            term: tr("admin-machine-links-open"),
            value: Value::Text(tr_args(
                "admin-of",
                [
                    ("open", (open).to_string().into()),
                    ("machines", (state.machines.len()).to_string().into()),
                ],
            )),
        },
        Fact {
            term: tr("admin-admin-pages"),
            value: Value::Code {
                shown: state.origin().to_owned(),
                exact: tr("admin-reachable-from-this-pc-only").into(),
            },
        },
        Fact {
            term: tr("admin-program-file"),
            value: path(state.setup.config()),
        },
        Fact {
            term: tr("admin-data-folder"),
            value: path(&state.directory),
        },
        Fact {
            term: tr("admin-times-on-these-pages"),
            value: Value::Text(format::time_zone()),
        },
    ];
    // Shown where Windows sign-in exists; Linux keeps the admin key only.
    if let Some(status) = state.windows.status() {
        facts.insert(
            3,
            Fact {
                term: tr("admin-windows-sign-in"),
                value: Value::Text(status.line()),
            },
        );
    }
    facts
}

/// The store's limits from the program file.
fn store(state: &Context) -> Vec<Fact> {
    let limits = &state.installed;
    vec![
        Fact {
            term: tr("admin-disk-space-kept-free"),
            value: size(state.reserve),
        },
        Fact {
            term: tr("admin-largest-database"),
            value: size(limits.database_bytes),
        },
        Fact {
            term: tr("admin-largest-message-or-record"),
            value: size(u64::try_from(limits.record_bytes).unwrap_or(u64::MAX)),
        },
        Fact {
            term: tr("admin-wait-for-a-busy-store"),
            value: limit(limits.busy_timeout),
        },
        Fact {
            term: tr("admin-store-start-limit"),
            value: limit(limits.startup_timeout),
        },
    ]
}

/// How long Analyte waits for itself: stopping, these pages and setup writes.
fn times(state: &Context) -> Vec<Fact> {
    vec![
        Fact {
            term: tr("admin-shutdown-limit"),
            value: limit(state.installed.shutdown),
        },
        Fact {
            term: tr("admin-page-reads"),
            value: limit(view::READ_BUDGET),
        },
        Fact {
            term: tr("admin-page-requests"),
            value: limit(crate::admin::server::REQUEST_BUDGET),
        },
        Fact {
            term: tr("admin-setup-changes"),
            value: limit(state.write_budget),
        },
    ]
}

/// How alerts go out: how often conditions are checked and, when a target
/// is set up, how its messages are sent; none says no target is set up.
fn alerts(state: &Context) -> Vec<Fact> {
    let installed = &state.installed;
    let mut facts = vec![Fact {
        term: tr("admin-checked-every"),
        value: limit(installed.alert_interval),
    }];
    if installed.alert_targets.is_empty() {
        facts.push(Fact {
            term: tr("admin-alert-targets"),
            value: Value::Text(tr("admin-none-set-up-alerts-show-on-these-pages-only").into()),
        });
        return facts;
    }
    facts.extend([
        Fact {
            term: tr("admin-alert-targets"),
            value: Value::Text(
                installed
                    .alert_targets
                    .iter()
                    .map(|(target, _)| target.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        },
        Fact {
            term: tr("admin-reminder-every"),
            value: limit(installed.alert_reminder),
        },
        Fact {
            term: tr("admin-wait-for-an-answer"),
            value: limit(installed.alert_timeout),
        },
        Fact {
            term: tr("admin-tries-per-alert"),
            value: Value::Text(installed.alert_attempts.to_string()),
        },
    ]);
    facts
}

/// Where each key is found: the admin key's file, then every lab app's and
/// alert target's keys by the service's own rule; never a value.
fn keys(state: &Context, found: Vec<crate::admin::setup::Found>) -> Vec<KeyRow> {
    let mut rows = vec![KeyRow {
        owner: tr("admin-admin-pages").into(),
        label: tr("admin-admin-key"),
        tone: Tone::Good,
        word: tr("admin-in-the-file-admin-token-in-the-data-folder").into(),
        exact: state.directory.join("admin-token").display().to_string(),
    }];
    rows.extend(found.into_iter().map(|found| KeyRow {
        owner: found.owner,
        label: found.label,
        tone: found.status.tone,
        word: found.status.word,
        exact: found.status.exact,
    }));
    rows
}
