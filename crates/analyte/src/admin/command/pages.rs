//! Prints Activity, active alerts, Data keeping and Cancelled by machine from the store, with each page's own reads and words.
//!
//! Each command opens the store as the other operator commands do, reads one
//! bounded page and closes the store on every path before printing. Reads
//! that show sample IDs (Activity, Cancelled by machine) are recorded in the
//! audit with the actor, as `orders list` is.
use crate::admin::error::CommandError;
use crate::admin::{Error, activity, more, overview::PrintedAlert};
use crate::installation::config::Program;
use analyte_model::{Identifier, Reason};
use analyte_store::{ActivityCursor, Audit, Handle};
use jiff::Timestamp;

/// Which Activity lines to print.
pub(super) struct Page {
    pub(super) machine: Option<Identifier>,
    pub(super) before: Option<ActivityCursor>,
    pub(super) limit: usize,
}

/// Opens the store, runs `work` on it and closes it on every path.
async fn with_store<T, F>(program: &Program, work: F) -> Result<T, Error>
where
    F: AsyncFnOnce(&Handle) -> Result<T, Error>,
{
    let mut store = crate::open_system_store(program).await?;
    let result = work(&store).await;
    store.shutdown(crate::deadline()?).await?;
    result
}

/// Records that `actor` read `what` from the command line.
async fn audit(store: &Handle, actor: Identifier, action: &str, why: &str) -> Result<(), Error> {
    let audit = Audit {
        actor,
        action: Identifier::new(action)?,
        targets: Vec::new(),
        time: Timestamp::now().as_second(),
        reason: Reason::new(why)?,
    };
    store
        .request::<_, CommandError>(crate::deadline()?, move |store| Ok(store.audit(&audit)?))
        .await?;
    Ok(())
}

/// `analyte activity`.
pub(super) async fn activity(
    program: &Program,
    actor: Identifier,
    page: Page,
    json: bool,
) -> Result<(), Error> {
    let models = super::models(program);
    let machine = page.machine.map(|machine| machine.as_str().to_owned());
    let (lines, next) = with_store(program, async |store| {
        audit(
            store,
            actor,
            "activity-list",
            "operator read the activity feed",
        )
        .await?;
        activity::printed(store, &models, machine, page.before, page.limit).await
    })
    .await?;
    if json {
        #[derive(serde::Serialize)]
        struct Shown<'a> {
            lines: &'a [activity::Printed],
            next: Option<&'a str>,
        }
        return super::json(&Shown {
            lines: &lines,
            next: next.as_deref(),
        });
    }
    if lines.is_empty() {
        super::say(if page.before.is_some() {
            "No earlier activity: this is past the oldest line Analyte keeps."
        } else {
            "No activity yet."
        })?;
    }
    for line in &lines {
        let at = line
            .local
            .as_deref()
            .unwrap_or(crate::admin::view::format::FEED_UNDATED);
        super::say(&format!("{at}  {:<12} {}", line.kind, line.text))?;
    }
    if let Some(next) = next {
        super::say(&format!("More: analyte activity --before {next}"))?;
    }
    Ok(())
}

/// `analyte alerts list`.
pub(super) async fn alerts(program: &Program, json: bool) -> Result<(), Error> {
    let alerts = with_store(program, async |store| {
        Ok(crate::operator::alerts::active(store).await?)
    })
    .await?;
    let now = Timestamp::now();
    let printed: Vec<_> = alerts
        .iter()
        .map(|alert| PrintedAlert::of(alert, now))
        .collect();
    if json {
        return super::json(&printed);
    }
    if printed.is_empty() {
        return super::say("No active alerts.");
    }
    for alert in &printed {
        let raised = alert.raised.as_deref().unwrap_or("not recorded");
        super::say(&format!(
            "{} ({}) · raised {raised} · for {}",
            alert.alert, alert.signal, alert.responder
        ))?;
        super::say(&format!("  {}", alert.reason))?;
        if let Some(fix) = &alert.fix {
            super::say(&format!("  {fix}"))?;
        }
        super::say(&format!("  Fix on: {}", alert.page))?;
        if let Some(command) = alert.command {
            super::say(&format!("  Command: {command}"))?;
        }
    }
    Ok(())
}

/// `analyte data-keeping`.
pub(super) async fn keeping(program: &Program, after: u64, json: bool) -> Result<(), Error> {
    let retention = crate::runtime::retention::policies(program);
    let page = with_store(program, async |store| {
        more::retention::printed(store, &retention, after).await
    })
    .await?;
    if json {
        return super::json(&page);
    }
    super::say("How long messages are kept")?;
    if page.kept.is_empty() {
        super::say("  No machine is set up.")?;
    }
    for (machine, kept) in &page.kept {
        super::say(&format!("  {machine}: {kept}"))?;
    }
    super::say(crate::admin::more::retention::LAST_CLEAN_UP)?;
    match &page.clean_up {
        Some(facts) => {
            for (term, value) in facts {
                super::say(&format!("  {term}: {value}"))?;
            }
        }
        None => super::say("  Not run yet")?,
    }
    super::say(&format!("Oldest message kept: {}", page.oldest))?;
    super::say(crate::admin::more::retention::HOLDS)?;
    if page.holds.is_empty() {
        super::say("  No holds: messages are deleted only as kept above.")?;
    }
    for hold in &page.holds {
        super::say(&format!("  {}. {}", hold.number, hold.keeps))?;
        super::say(&format!("     {}", hold.placed))?;
        super::say(&format!("     {}", hold.target))?;
    }
    if let Some(next) = page.next {
        super::say(&format!("More: analyte data-keeping --after {next}"))?;
    }
    Ok(())
}

/// `analyte orders cancelled`.
pub(super) async fn cancelled(
    program: &Program,
    actor: Identifier,
    after: u64,
    json: bool,
) -> Result<(), Error> {
    let models = super::models(program);
    let (items, next) = with_store(program, async |store| {
        audit(
            store,
            actor,
            "orders-cancelled-list",
            "operator read orders cancelled by a machine",
        )
        .await?;
        more::awos::printed(store, &models, after).await
    })
    .await?;
    if json {
        #[derive(serde::Serialize)]
        struct Shown<'a> {
            items: &'a [more::awos::Printed],
            next: Option<i64>,
        }
        return super::json(&Shown {
            items: &items,
            next,
        });
    }
    if items.is_empty() {
        super::say("No orders cancelled by a machine.")?;
    }
    for item in &items {
        let title = item.sample.as_ref().map_or_else(
            || format!("Order {}", item.order),
            |sample| format!("Sample {sample} · order {}", item.order),
        );
        let machine = item.model.as_ref().map_or_else(
            || item.machine.clone(),
            |model| format!("{} ({model})", item.machine),
        );
        let tests = item
            .tests
            .as_ref()
            .map_or_else(String::new, |tests| format!(" · {tests}"));
        super::say(&format!("{title} · {machine}{tests}"))?;
        for line in &item.lines {
            super::say(&format!("  {line}"))?;
        }
        super::say(&format!("  {}", item.identity))?;
    }
    if let Some(next) = next {
        super::say(&format!("More: analyte orders cancelled --after {next}"))?;
    }
    Ok(())
}

/// Confirms a known software warning from the command line.
pub(super) async fn confirm_software(
    program: &Program,
    actor: Identifier,
    machine: Identifier,
    version: Identifier,
) -> Result<(), Error> {
    let check = program
        .machines()
        .iter()
        .find(|entry| *entry.id() == machine)
        .and_then(|entry| entry.snapshot().effective().machine().software_version())
        .and_then(analyte_config::software_version::Check::expected)
        .ok_or(Error::NoSoftwareCheck)?;
    let revision = check.revision;
    with_store(program, async |store| {
        let id = machine.clone();
        let warning = store
            .read::<_, CommandError>(crate::deadline()?, move |store| {
                Ok(store.software_warning(&id, revision)?)
            })
            .await?
            .ok_or(Error::NoSoftwareWarning)?;
        crate::admin::machines::software_version::confirm(
            store,
            machine,
            revision,
            version,
            warning.first_seen,
            actor,
        )
        .await
    })
    .await?;
    super::say("Settings checked for this version.")
}
