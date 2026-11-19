//! Says which results wait for a lab app destination that no running machine uses, and what happens to them.
//!
//! A destination kept after its last machine was removed gets a delivery-only
//! worker at startup that runs until nothing is left (`runtime::delivery::retired`).
//! A destination that is not kept anywhere cannot be delivered to: Analyte never
//! guesses an address, so its items stay stored and this page says so.
use super::server::Context;
use crate::admin::Error;
use analyte_store::Undelivered;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Each kept destination's flag, by destination name: true while its
/// delivery-only worker runs, false once it sent everything or stopped.
pub(crate) type Draining = BTreeMap<String, Arc<AtomicBool>>;

/// One plain sentence per destination that holds items the lab app has not
/// accepted and that no running machine uses; none when nothing waits. Names and
/// counts only, never payloads or sample IDs (safety rule 10).
pub(super) async fn lines(state: &Context) -> Result<Vec<String>, Error> {
    let open = state
        .store
        .request(crate::deadline()?, |store| {
            store.undelivered_by_destination()
        })
        .await?;
    Ok(open
        .iter()
        .filter(|open| {
            !state
                .destinations
                .iter()
                .any(|served| served.id() == &open.destination)
        })
        .map(|open| {
            let running = |name: &analyte_model::Identifier| {
                state.machines.iter().any(|machine| &machine.id == name)
            };
            line(
                open,
                state.retired.get(open.destination.as_str()),
                open.machines.iter().any(running),
            )
        })
        .collect())
}

fn line(open: &Undelivered, kept: Option<&Arc<AtomicBool>>, from_running: bool) -> String {
    let count = open.sending.saturating_add(open.held);
    let what = if count == 1 {
        "result or order message"
    } else {
        "results or order messages"
    };
    let destination = open.destination.as_str();
    let from = match (open.machines.len(), from_running) {
        (0, _) => format!("Lab app destination {destination}"),
        (1, false) => format!("Removed machine {}", names(&open.machines)),
        (_, false) => format!("Removed machines {}", names(&open.machines)),
        (1, true) => format!("Machine {}", names(&open.machines)),
        (_, true) => format!("Machines {}", names(&open.machines)),
    };
    let held = if open.held > 0 {
        format!(
            " {} of them wait for an operator on the Deliveries page (paused, parked or conflicting).",
            open.held
        )
    } else {
        String::new()
    };
    match kept {
        Some(flag) if flag.load(Ordering::Acquire) => format!(
            "{from}: {count} {what} still being delivered to lab app destination {destination}.{held}"
        ),
        Some(_) => format!(
            concat!(
                "{from}: {count} {what} wait for lab app destination {destination}; its ",
                "delivery-only worker had already finished, so they are delivered after the ",
                "next restart.{held}"
            ),
            from = from,
            count = count,
            what = what,
            destination = destination,
            held = held
        ),
        None => format!(
            concat!(
                "Cannot deliver: destination {destination} no longer configured. {from}: ",
                "{count} {what} stay stored and are never deleted. Set up a machine that uses ",
                "this lab app destination to deliver them."
            ),
            destination = destination,
            from = from,
            count = count,
            what = what
        ),
    }
}

fn names(machines: &[analyte_model::Identifier]) -> String {
    let mut text = machines
        .iter()
        .map(analyte_model::Identifier::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if machines.len() >= analyte_store::UNDELIVERED_MACHINES {
        text.push_str(" and possibly more");
    }
    text
}
