//! Holds the fixed wording pages share, so each label, statement and piece of advice is written once.

/// The title every page gives order sending paused after a restore.
pub(in crate::admin) const RESTORE_PAUSED: &str = "Order sending is paused after a restore";

/// What someone does while order sending is paused after a restore.
pub(in crate::admin) const RESTORE_ADVICE: &str =
    "Ask IT to reconcile orders with the command line; until then no order is sent to a machine.";

/// What a restart does to the machines, said only as far as Analyte knows it:
/// whether a machine sends again what was not acknowledged depends on the machine.
pub(in crate::admin) const RESTART_ADVICE: &str = concat!(
    "Restarting stops every machine connection for a few seconds, and Analyte acknowledges ",
    "nothing while it is stopped. After the restart, sign in again.",
);

/// The warning above original bytes or a machine's free text, once shown.
pub(in crate::admin) const PATIENT_DATA: &str = "May contain patient data";

/// What the warning above original bytes or free text says.
pub(in crate::admin) const PATIENT_DATA_TEXT: &str = concat!(
    "Keep it out of screenshots and support tickets. Analyte recorded who asked to see it and ",
    "why before showing it.",
);

/// The title of the exact IDs and reasons kept for support, on every page.
pub(in crate::admin) const SUPPORT: &str = "Details for support";

/// A value Analyte never stored, such as a time an older version did not keep.
pub(in crate::admin) const NOT_RECORDED: &str = "Not recorded";

pub(in crate::admin) use crate::admin::view::status_words::NOTHING_YET;

/// A value that does not apply here.
pub(in crate::admin) const NOT_APPLICABLE: &str = "—";

/// What a page says when its reads took longer than
/// [`super::super::READ_BUDGET`].
pub(in crate::admin) const SLOW_READ: &str = "Analyte did not finish reading within 1.5 seconds.";

/// What a machine's status says once Analyte stopped working with it.
pub(in crate::admin) const STOPPED: &str = "Stopped in Analyte";

/// What Analyte observed about a machine it stopped working with.
pub(in crate::admin) const STOPPED_SENTENCE: &str = "Analyte stopped working with this machine";

/// What removing a machine does, said before and after.
pub(in crate::admin) const REMOVED_ADVICE: &str = concat!(
    "After the next restart Analyte stops connecting to it; until then it keeps running. ",
    "Its results, items to review and history are kept and stay visible here.",
);

/// What to do for a machine the settings list without a model.
pub(in crate::admin) const SET_UP_ADVICE: &str = concat!(
    "set it up: pick its model, how it is connected and which lab app gets its results, ",
    "run a listen-only test, then save and restart.",
);

/// What to check when a lab app's tries keep failing.
pub(in crate::admin) const FAILING_ADVICE: &str =
    "Check that the lab app is running and that this PC can reach it at its address.";

/// When keys entered on the Lab apps page take effect.
pub(in crate::admin) const KEYS_APPLY: &str =
    "Waiting items go out after Analyte restarts with the keys.";

/// What to do for a lab app Analyte cannot deliver to.
pub(in crate::admin) const KEYS_ADVICE: &str = concat!(
    "add the missing keys on the Lab apps page or in the service environment. ",
    "Waiting items go out after Analyte restarts with the keys."
);

/// Said under a key's field when the key source holds no value for it.
pub(in crate::admin) const KEY_NEEDED: &str = "Needed before Analyte can send results.";

/// Said under a key's field when the locked keys file could not be read.
pub(in crate::admin) const KEY_UNCHECKED: &str = "The locked keys file could not be read, so this key cannot be checked; the reason is under Details for support.";

/// Heads reports where an action made no changes.
pub(in crate::admin) const NOTHING_CHANGED: &str = "Nothing was changed";

/// Labels the action to restart the engine to apply settings.
pub(in crate::admin) const RESTART_NOW: &str = "Restart now";

/// Labels the link out of an unavailable page of a list.
pub(in crate::admin) const FIRST_PAGE: &str = "Back to the first page";
