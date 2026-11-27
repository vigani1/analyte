//! Shows and confirms one exact software warning for a running machine.
use crate::admin::Error;
use crate::admin::error::CommandError;
use crate::admin::server::{Context, sessions::Session};
use crate::admin::view::{
    Tone, format,
    report::{Problem, Section},
};
use analyte_config::software_version::{Check, ExpectedVersions};
use analyte_model::{Digest, Identifier, Reason};
use analyte_store::{Audit, Handle};
use axum::{
    Extension, Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use jiff::Timestamp;
use std::sync::Arc;

pub(super) struct Panel {
    pub(super) tone: Tone,
    pub(super) text: String,
    pub(super) first_seen: Timestamp,
    pub(super) confirm: Option<Confirmation>,
}

pub(super) struct Confirmation {
    pub(super) action: String,
    pub(super) csrf: String,
    pub(super) version: Identifier,
}

pub(super) async fn detail(
    state: &Context,
    session: Option<&Session>,
    id: &str,
) -> Result<Option<Panel>, Error> {
    let Some(machine) = state.machines.iter().find(|entry| entry.id.as_str() == id) else {
        return Ok(None);
    };
    let Some(check) = machine.software.as_ref().and_then(Check::expected) else {
        return Ok(None);
    };
    let machine_id = machine.id.clone();
    let revision = check.revision;
    let warning = state
        .store
        .read::<_, CommandError>(crate::deadline()?, move |store| {
            Ok(store.software_warning(&machine_id, revision)?)
        })
        .await?;
    Ok(warning.map(|warning| Panel {
        tone: Tone::Attention,
        text: crate::machine::software_version::message(&machine.id, check, &warning),
        first_seen: warning.first_seen,
        confirm: session
            .zip(warning.reported)
            .map(|(session, version)| Confirmation {
                action: format!("{}/confirm-software", format::machine_href(id)),
                csrf: session.csrf.form_value().to_owned(),
                version,
            }),
    }))
}

/// Form identity comes from the reviewed warning. The session supplies the actor.
#[derive(serde::Deserialize)]
pub(in crate::admin) struct Submitted {
    version: Identifier,
    first_seen: Timestamp,
}

pub(in crate::admin) async fn submit(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(id): Path<String>,
    Form(form): Form<Submitted>,
) -> Response {
    let Some(machine) = state.machines.iter().find(|entry| entry.id.as_str() == id) else {
        return Problem::NotFound.page(&state, Some(&session), Section::Machines(None));
    };
    let refusal =
        |text| Problem::Refused(text).page(&state, Some(&session), Section::Machines(Some(&id)));
    let (Some(check), Some(actor)) = (
        machine.software.as_ref().and_then(Check::expected),
        session.actor(),
    ) else {
        return refusal("This machine has no software warning you can confirm.".into());
    };
    match confirm(
        &state.store,
        machine.id.clone(),
        check.revision,
        form.version,
        form.first_seen,
        actor.clone(),
    )
    .await
    {
        Ok(()) => Redirect::to(&format::machine_href(&id)).into_response(),
        Err(error) => refusal(error.to_string()),
    }
}

/// Applies a confirmation for the exact warning and appends the operator audit.
pub(in crate::admin) async fn confirm(
    store: &Handle,
    machine: Identifier,
    revision: Digest,
    version: Identifier,
    first_seen: Timestamp,
    actor: Identifier,
) -> Result<(), Error> {
    let audit = Audit {
        actor,
        action: Identifier::new("settings-checked-for-software-version")?,
        targets: vec![machine.clone(), version.clone()],
        time: Timestamp::now().as_second(),
        reason: Reason::new("Settings checked for this version")?,
    };
    store
        .request::<_, CommandError>(crate::deadline()?, move |store| {
            Ok(store.confirm_software_warning(&machine, revision, &version, first_seen, &audit)?)
        })
        .await?;
    Ok(())
}

/// Reads warnings for the explicitly active checks in a single store request.
pub(in crate::admin) async fn warnings(
    store: &Handle,
    until: std::time::Instant,
    checks: Vec<(Identifier, ExpectedVersions)>,
) -> Result<Vec<(Identifier, String)>, Error> {
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    Ok(store
        .read::<_, CommandError>(until, move |store| {
            let mut warnings = Vec::new();
            for (machine, check) in checks {
                if let Some(warning) = store.software_warning(&machine, check.revision)? {
                    let text =
                        crate::machine::software_version::message(&machine, &check, &warning);
                    warnings.push((machine, text));
                }
            }
            Ok(warnings)
        })
        .await?)
}
