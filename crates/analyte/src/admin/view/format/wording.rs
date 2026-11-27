//! Holds the fixed wording pages share, so each label, statement and piece of advice is written once.

use analyte_i18n::text as tr;

/// The title every page gives order sending paused after a restore.
pub(in crate::admin) fn restore_paused() -> &'static str {
    tr("admin-restore-paused")
}

/// What someone does while order sending is paused after a restore.
pub(in crate::admin) fn restore_advice() -> &'static str {
    tr("admin-restore-advice")
}

/// What a restart does to the machines, said only as far as Analyte knows it:
/// whether a machine sends again what was not acknowledged depends on the machine.
pub(in crate::admin) fn restart_advice() -> &'static str {
    tr("admin-restart-advice")
}

/// The warning above original bytes or a machine's free text, once shown.
pub(in crate::admin) fn patient_data() -> &'static str {
    tr("admin-patient-data")
}

/// What the warning above original bytes or free text says.
pub(in crate::admin) fn patient_data_text() -> &'static str {
    tr("admin-patient-data-text")
}

/// The title of the exact IDs and reasons kept for support, on every page.
pub(in crate::admin) fn support() -> &'static str {
    tr("admin-support")
}

/// A value Analyte never stored, such as a time an older version did not keep.
pub(in crate::admin) fn not_recorded() -> &'static str {
    tr("admin-not-recorded")
}

/// A value for something that has not happened yet.
pub(in crate::admin) fn nothing_yet() -> &'static str {
    tr("admin-nothing-yet")
}

/// A value that does not apply here.
pub(in crate::admin) fn not_applicable() -> &'static str {
    tr("admin-not-applicable")
}

/// What a page says when its reads took longer than
/// [`super::super::READ_BUDGET`].
pub(in crate::admin) fn slow_read() -> &'static str {
    tr("admin-slow-read")
}

/// What a machine's status says once Analyte stopped working with it.
pub(in crate::admin) fn stopped() -> &'static str {
    tr("admin-stopped")
}

/// What Analyte observed about a machine it stopped working with.
pub(in crate::admin) fn stopped_sentence() -> &'static str {
    tr("admin-stopped-sentence")
}

/// What removing a machine does, said before and after.
pub(in crate::admin) fn removed_advice() -> &'static str {
    tr("admin-removed-advice")
}

/// What to do for a machine the settings list without a model.
pub(in crate::admin) fn set_up_advice() -> &'static str {
    tr("admin-set-up-advice")
}

/// What to check when a lab app's tries keep failing.
pub(in crate::admin) fn failing_advice() -> &'static str {
    tr("admin-failing-advice")
}

/// When keys entered on the Lab apps page take effect.
pub(in crate::admin) fn keys_apply() -> &'static str {
    tr("admin-keys-apply")
}

/// What to do for a lab app Analyte cannot deliver to.
pub(in crate::admin) fn keys_advice() -> &'static str {
    tr("admin-keys-advice")
}

/// Said under a key's field when the key source holds no value for it.
pub(in crate::admin) fn key_needed() -> &'static str {
    tr("admin-key-needed")
}

/// Said under a key's field when the locked keys file could not be read.
pub(in crate::admin) fn key_unchecked() -> &'static str {
    tr("admin-key-unchecked")
}

/// Heads reports where an action made no changes.
pub(in crate::admin) fn nothing_changed() -> &'static str {
    tr("admin-nothing-changed")
}

/// Labels the action to restart the engine to apply settings.
pub(in crate::admin) fn restart_now() -> &'static str {
    tr("admin-restart-now")
}

/// Labels the link out of an unavailable page of a list.
pub(in crate::admin) fn first_page() -> &'static str {
    tr("admin-first-page")
}
