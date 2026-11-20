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
use crate::admin::view::Tone;
use analyte_model::Identifier;
use analyte_store::{Undeliverable, Undelivered};
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
    /// The credential names of its API key and signing key, so the overview can
    /// ask the key source which one is missing.
    pub(crate) api_key: Identifier,
    pub(crate) signing_key: Identifier,
}

/// One sentence about results waiting for a destination no running machine uses.
pub(super) struct Line {
    text: String,
    destination: Identifier,
    waiting: u64,
    /// Why nothing reaches the lab app until someone acts; none while delivery can go on.
    blocked: Option<Undeliverable>,
    /// Who sent the waiting items, that they are kept and how many need
    /// someone on the Deliveries page; only for a lab app whose keys are
    /// missing, whose node on the Overview's signal path says the rest.
    kept: Option<String>,
}

impl Line {
    /// A problem when nothing can be delivered, otherwise a warning.
    pub(super) const fn tone(&self) -> Tone {
        if self.blocked.is_some() {
            Tone::Problem
        } else {
            Tone::Attention
        }
    }

    /// The lab app the waiting items are for.
    pub(super) const fn destination(&self) -> &Identifier {
        &self.destination
    }

    /// The sentence itself.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The lab app, why nothing can be delivered to it, how many items wait
    /// and, when its node shows the rest ([`Self::on_path`]), what only this
    /// line says; none while delivery can go on.
    pub(super) fn blocked(&self) -> Option<(Identifier, Undeliverable, u64, Option<String>)> {
        self.blocked.map(|reason| {
            (
                self.destination.clone(),
                reason,
                self.waiting,
                self.kept.clone(),
            )
        })
    }

    /// A kept lab app whose keys are missing has its own node on the
    /// Overview's signal path, which says it cannot deliver, which keys are
    /// missing and what to do; there the sentence would only repeat it.
    pub(super) const fn on_path(&self) -> bool {
        matches!(self.blocked, Some(Undeliverable::CredentialsMissing))
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
            let running =
                |name: &Identifier| state.machines.iter().any(|machine| &machine.id == name);
            line(
                open,
                state.retired.get(open.destination.as_str()),
                open.machines.iter().any(running),
            )
        })
        .collect())
}

fn line(open: &Undelivered, kept: Option<&Kept>, from_running: bool) -> Line {
    let count = open.waiting();
    let what = if count == 1 {
        "result or order message"
    } else {
        "results or order messages"
    };
    let destination = open.destination.as_str();
    let from = match (open.machines.len(), from_running) {
        (0, _) => format!("Lab app {destination}"),
        (1, false) => format!("Removed machine {}", names(&open.machines)),
        (_, false) => format!("Removed machines {}", names(&open.machines)),
        (1, true) => format!("Machine {}", names(&open.machines)),
        (_, true) => format!("Machines {}", names(&open.machines)),
    };
    let held = if open.held > 0 {
        format!(
            " {} of them need someone to act on the Deliveries page (paused, rejected or in conflict).",
            open.held
        )
    } else {
        String::new()
    };
    let blocked = crate::operator::alerts::undeliverable_reason(
        kept.is_some(),
        kept.is_some_and(|kept| kept.keys_missing),
    );
    let are = if count == 1 { "is" } else { "are" };
    let stays = format!("{from}: {count} {what} {are} kept and never deleted.{held}");
    let text = match blocked {
        // The worker runs, but without keys nothing is sent (ADR 0014 alert text).
        Some(Undeliverable::CredentialsMissing) => format!(
            concat!(
                "Cannot send to lab app {destination}: its API key or signing key is ",
                "missing. Enter them on the Lab apps page (or in the service environment), ",
                "then restart. {stays}"
            ),
            destination = destination,
            stays = stays,
        ),
        Some(Undeliverable::NotConfigured) => format!(
            concat!(
                "Cannot send to lab app {destination}: it is not set up any more. {from}: ",
                "{count} {what} {are} kept and never deleted. Add lab app {destination} on the ",
                "Lab apps page, then restart, to send them."
            ),
            destination = destination,
            from = from,
            count = count,
            what = what,
            are = are
        ),
        None if kept.is_some_and(|kept| kept.delivering.load(Ordering::Acquire)) => {
            format!("{from}: {count} {what} still being delivered to lab app {destination}.{held}")
        }
        None => format!(
            concat!(
                "{from}: {count} {what} wait for lab app {destination}; they are sent after ",
                "the next restart.{held}"
            ),
            from = from,
            count = count,
            what = what,
            destination = destination,
            held = held
        ),
    };
    Line {
        text,
        destination: open.destination.clone(),
        waiting: count,
        blocked,
        kept: matches!(blocked, Some(Undeliverable::CredentialsMissing)).then_some(stays),
    }
}

fn names(machines: &[Identifier]) -> String {
    let mut text = machines
        .iter()
        .map(Identifier::as_str)
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
            api_key: Identifier::new("old-lab-api").expect("valid test name"),
            signing_key: Identifier::new("old-lab-signing").expect("valid test name"),
        }
    }

    // ADR 0003 rule 2 and ADR 0014: a kept destination whose keys are missing
    // has a running worker that sends nothing, so the page must not say "still
    // being delivered"; it gives the alert's reason and fix instead.
    #[test]
    fn kept_destination_without_keys_says_cannot_deliver() -> Result<(), analyte_model::TextError> {
        let open = waiting(&["machine-3"])?;
        let line = line(&open, Some(&kept(true, true)), false);
        assert_eq!(
            line.blocked(),
            Some((
                Identifier::new("old-lab")?,
                Undeliverable::CredentialsMissing,
                3,
                Some("Removed machine machine-3: 3 results or order messages are kept and never deleted.".into())
            ))
        );
        assert!(line.on_path());
        assert!(
            !line.text.contains("still being delivered"),
            "{}",
            line.text
        );
        assert!(line.text.starts_with(concat!(
            "Cannot send to lab app old-lab: its API key or signing key is missing. ",
            "Enter them on the Lab apps page (or in the service environment), then restart. ",
            "Removed machine machine-3: 3 results or order messages are kept and never deleted."
        )));
        assert_eq!(line.tone(), Tone::Problem);
        Ok(())
    }

    // "Still being delivered" only while delivery is actually possible.
    #[test]
    fn kept_destination_with_keys_is_still_being_delivered() -> Result<(), analyte_model::TextError>
    {
        let open = waiting(&["machine-3"])?;
        let line = line(&open, Some(&kept(true, false)), false);
        assert_eq!(line.blocked(), None);
        assert_eq!(
            line.text,
            "Removed machine machine-3: 3 results or order messages still being delivered to lab app old-lab."
        );
        assert_eq!(line.tone(), Tone::Attention);
        let finished = line_text(&open, Some(&kept(false, false)));
        assert!(
            finished.contains("sent after the next restart"),
            "{finished}"
        );
        let gone = super::line(&open, None, false);
        assert!(
            gone.text
                .starts_with("Cannot send to lab app old-lab: it is not set up any more.")
        );
        // No lab app node says this, so the sentence stays on the Overview.
        assert!(!gone.on_path());
        assert_eq!(gone.blocked().and_then(|blocked| blocked.3), None);
        Ok(())
    }

    fn line_text(open: &Undelivered, kept: Option<&Kept>) -> String {
        line(open, kept, false).text
    }
}
