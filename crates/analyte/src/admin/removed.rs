//! Says which results wait for a lab app destination that no running machine uses, and what happens to them.
//!
//! A lab app no running machine uses (listed in `lab-apps.toml`, or kept by an
//! older setup page) gets a delivery-only worker at startup that runs until
//! nothing is left (`runtime::delivery::unused_lab_apps`). A destination that is
//! no lab app cannot be delivered to: Analyte never guesses an address, so its
//! items stay stored and this page says so. Neither can a lab app whose keys
//! were missing at startup: its worker runs but
//! sends nothing, so the page gives the same reason and fix as the
//! `undeliverable_destination` alert instead of "still being delivered".
use super::server::Context;
use crate::admin::Error;
use crate::admin::markup::Markup;
use analyte_store::Undelivered;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Each kept destination's delivery state, by destination name.
pub(crate) type Draining = BTreeMap<String, Kept>;

/// What startup knows about one kept destination's delivery-only worker.
#[derive(Clone, Debug)]
pub(crate) struct Kept {
    /// True while its worker runs, false once it sent everything or stopped.
    pub(crate) delivering: Arc<AtomicBool>,
    /// Its API key or signing key could not be loaded at startup, so its worker
    /// can send nothing until the keys are entered and Analyte restarts.
    pub(crate) keys_missing: bool,
}

/// One sentence about results waiting for a destination no running machine uses.
pub(super) struct Line {
    text: String,
    /// Nothing reaches the lab app until someone acts.
    blocked: bool,
}

impl Line {
    /// The sentence in a warning box: red when nothing can be delivered.
    pub(super) fn notice(&self, label: &'static str) -> Markup {
        Markup::new()
            .markup(if self.blocked {
                "<p class=\"notice bad\">"
            } else {
                "<p class=\"notice\">"
            })
            .markup(label)
            .text(&self.text)
            .markup("</p>")
    }
}

/// One plain sentence per destination that holds items the lab app has not
/// accepted and that no running machine uses; none when nothing waits. Names and
/// counts only, never payloads or sample IDs (safety rule 10).
pub(super) async fn lines(state: &Context) -> Result<Vec<Line>, Error> {
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

fn line(open: &Undelivered, kept: Option<&Kept>, from_running: bool) -> Line {
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
    let (text, blocked) = match kept {
        // The worker runs, but without keys nothing is sent (ADR 0014 alert text).
        Some(kept) if kept.keys_missing => (
            format!(
                concat!(
                    "Cannot deliver to lab app {destination}: its API key or signing key is ",
                    "missing. Enter them on the Lab app page or in ",
                    "the service environment, then restart. {from}: {count} {what} stay stored ",
                    "and are never deleted.{held}"
                ),
                destination = destination,
                from = from,
                count = count,
                what = what,
                held = held
            ),
            true,
        ),
        Some(kept) if kept.delivering.load(Ordering::Acquire) => (
            format!(
                "{from}: {count} {what} still being delivered to lab app destination {destination}.{held}"
            ),
            false,
        ),
        Some(_) => (
            format!(
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
            false,
        ),
        None => (
            format!(
                concat!(
                    "Cannot deliver: destination {destination} no longer configured. {from}: ",
                    "{count} {what} stay stored and are never deleted. Add lab app {destination} ",
                    "on the Lab app page and restart to deliver them."
                ),
                destination = destination,
                from = from,
                count = count,
                what = what
            ),
            true,
        ),
    };
    Line { text, blocked }
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

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::Identifier;

    fn waiting(machines: &[&str]) -> Result<Undelivered, analyte_model::TextError> {
        Ok(Undelivered {
            destination: Identifier::new("old-lab")?,
            sending: 3,
            held: 0,
            machines: machines
                .iter()
                .map(|name| Identifier::new(name))
                .collect::<Result<_, _>>()?,
        })
    }

    fn kept(delivering: bool, keys_missing: bool) -> Kept {
        Kept {
            delivering: Arc::new(AtomicBool::new(delivering)),
            keys_missing,
        }
    }

    // ADR 0003 rule 2 and ADR 0014: a kept destination whose keys are missing
    // has a running worker that sends nothing, so the page must not say "still
    // being delivered"; it gives the alert's reason and fix instead.
    #[test]
    fn kept_destination_without_keys_says_cannot_deliver() -> Result<(), analyte_model::TextError> {
        let open = waiting(&["machine-3"])?;
        let line = line(&open, Some(&kept(true, true)), false);
        assert!(line.blocked);
        assert!(
            !line.text.contains("still being delivered"),
            "{}",
            line.text
        );
        assert!(line.text.starts_with(concat!(
            "Cannot deliver to lab app old-lab: its API key or signing key is missing. ",
            "Enter them on the Lab app page or in the service ",
            "environment, then restart. Removed machine machine-3: 3 results or order ",
            "messages stay stored and are never deleted."
        )));
        assert!(line.notice("").starts_with("<p class=\"notice bad\">"));
        Ok(())
    }

    // "Still being delivered" only while delivery is actually possible.
    #[test]
    fn kept_destination_with_keys_is_still_being_delivered() -> Result<(), analyte_model::TextError>
    {
        let open = waiting(&["machine-3"])?;
        let line = line(&open, Some(&kept(true, false)), false);
        assert!(!line.blocked);
        assert_eq!(
            line.text,
            "Removed machine machine-3: 3 results or order messages still being delivered to lab app destination old-lab."
        );
        assert!(line.notice("").starts_with("<p class=\"notice\">"));
        let finished = line_text(&open, Some(&kept(false, false)));
        assert!(
            finished.contains("delivered after the next restart"),
            "{finished}"
        );
        let gone = line_text(&open, None);
        assert!(gone.starts_with("Cannot deliver: destination old-lab no longer configured."));
        Ok(())
    }

    fn line_text(open: &Undelivered, kept: Option<&Kept>) -> String {
        line(open, kept, false).text
    }
}
