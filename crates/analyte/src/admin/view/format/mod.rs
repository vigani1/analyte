//! Turns technical values into words a lab manager reads at a glance; pages keep the exact value beside them.
//!
//! Sizes use binary units (1 GB = 1024³ bytes), the way Windows Explorer shows a
//! disk. Times show in the PC's own time zone; the exact UTC time and Unix
//! seconds stay in the hover text. Connection sentences say only what Analyte
//! observed (a port open or not, a listener with or without a peer), never what
//! it cannot see, such as a cable or a machine being switched on.
mod activity;
mod history;
mod href;
mod lab_app;
mod look;
mod machine;
mod parked;
mod time;
mod wording;

use analyte_i18n::{count as tr_count, text as tr};

pub(in crate::admin) use activity::{Named, Said, events, feed_day, feed_time, feed_undated, said};
pub(in crate::admin) use history::{
    actor, arrived, outcomes, recorded, settled, settled_action, settled_line,
};
pub(in crate::admin) use href::{
    lab_app_href, machine_href, order_item_href, parked_href, path_segment, setup_href,
};
pub(in crate::admin) use lab_app::{
    failing, lab_app, last_delivered, missing_keys, nothing_waiting, send_again, senders,
    set_in_environment, set_on, waiting, waiting_to_send,
};
pub(in crate::admin) use look::{StateLook, attempt_look, delivery_look, execution, order_look};
pub(in crate::admin) use machine::{
    MachineCounts, accepts, accepts_from, add_machine, analyzer_settings, any_device,
    cable_settings, connected_by, connection, connection_detail, connection_kind, connection_note,
    control_names, deliveries, departure, driver, in_analyte, last_data, link_advice,
    link_sentence, machine, machine_lede, machines_summary, model, no_machine_running,
    no_machine_set_up, not_chosen, not_running, not_set_up, nothing_received_yet, only_devices,
    open_machine, open_to_any, open_to_any_advice, reading, recording_counts, recording_only,
    refused, refused_advice, samples, sends_results_to, settings_unreadable, starts_after_restart,
    stops_at_restart,
};
pub(in crate::admin) use parked::{
    as_arrived, compare_measurements, explain, how_read, linked_record, mark_resolved,
    marked_resolved, open_item, original_message, poll_title, process_again, processed_again,
    reason_words, show_original,
};
pub(in crate::admin) use time::{
    Line, Moment, Part, ago, clock, countdown, duration, exact_time, limit, live_clock, local_time,
    moment, moment_unix, page_loaded, time_zone, today, today_span,
};
pub(in crate::admin) use wording::{
    failing_advice, first_page, key_needed, key_unchecked, keys_advice, keys_apply, not_applicable,
    not_recorded, nothing_changed, nothing_yet, patient_data, patient_data_text, removed_advice,
    restart_advice, restart_now, restore_advice, restore_paused, set_up_advice, slow_read, stopped,
    stopped_sentence, support,
};

/// A size such as "477 GB" or "1.5 MB"; under 1 KB it counts bytes.
pub(in crate::admin) fn bytes(count: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if count < 1024 {
        return tr_count("admin-count-bytes", count);
    }
    let value = u128::from(count);
    let mut unit: u128 = 1024;
    let mut name = UNITS[0];
    // Move up while the value, rounded, would read 1024 or more of this unit.
    for next in UNITS.iter().skip(1) {
        if rounded(value, unit) < 1024 {
            break;
        }
        unit = unit.saturating_mul(1024);
        name = next;
    }
    let tenths = rounded(value.saturating_mul(10), unit);
    if tenths < 100 && !tenths.is_multiple_of(10) {
        format!("{}.{} {name}", tenths / 10, tenths % 10)
    } else {
        format!("{} {name}", rounded(value, unit))
    }
}

/// `value / unit` rounded half up; `unit` is a power of 1024, never zero.
fn rounded(value: u128, unit: u128) -> u128 {
    value
        .saturating_add(unit / 2)
        .checked_div(unit)
        .unwrap_or_default()
}

/// "5 machines · 2 lab apps", beside a page title.
pub(in crate::admin) fn machines_and_lab_apps(machines: usize, lab_apps: usize) -> String {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    format!(
        "{} · {}",
        tr_count("admin-count-machines", count(machines)),
        tr_count("admin-count-lab-apps", count(lab_apps))
    )
}

/// A settings version in full: the configuration digest in lowercase hex.
pub(in crate::admin) fn digest_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The short settings version people compare at a glance: the digest's first 12 hex digits.
pub(in crate::admin) fn digest_short(digest: &[u8]) -> String {
    digest_hex(digest).chars().take(12).collect()
}

/// Words written as one sentence: the first letter upper case and a full stop
/// at the end, such as a setup refusal shown on its own.
pub(in crate::admin) fn sentence(words: &str) -> String {
    let mut chars = words.chars();
    let mut text: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if !text.ends_with('.') {
        text.push('.');
    }
    text
}

/// "Yes" or "No".
pub(in crate::admin) fn yes_no(value: bool) -> &'static str {
    if value {
        tr("setup-yes")
    } else {
        tr("setup-no")
    }
}

/// How many tests an order or item covers: "1 test", "4 tests"; never which.
pub(in crate::admin) fn test_count(count: usize) -> String {
    tr_count(
        "admin-count-tests",
        u64::try_from(count).unwrap_or(u64::MAX),
    )
}

/// Describes a count when there is something to list.
pub(in crate::admin) fn nonzero_count(id: &str, count: u64) -> Option<String> {
    (count > 0).then(|| tr_count(id, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner request 2026-11-19: sizes read like a disk, with the exact bytes kept elsewhere.
    #[test]
    fn sizes_use_the_largest_readable_unit() {
        for (count, expected) in [
            (0, "0 bytes"),
            (1, "1 byte"),
            (2, "2 bytes"),
            (1_023, "1023 bytes"),
            (1_024, "1 KB"),
            (1_536, "1.5 KB"),
            (1_048_575, "1 MB"),
            (1_073_741_824, "1 GB"),
            (10 * 1_073_741_824, "10 GB"),
            (511_966_605_312, "477 GB"),
            (u64::MAX, "16 EB"),
        ] {
            assert_eq!(bytes(count), expected, "{count}");
        }
    }

    // Owner decision 2026-11-20 (admin restyle b2): a setup refusal reads as
    // one sentence on its page, with its words unchanged.
    #[test]
    fn refusals_read_as_one_sentence() {
        assert_eq!(
            sentence("no installed machine is called x"),
            "No installed machine is called x."
        );
        assert_eq!(sentence("Already a sentence."), "Already a sentence.");
    }

    #[test]
    fn counts_agree_with_their_word() {
        assert_eq!(tr_count("admin-count-results", 0u64), "0 results");
        assert_eq!(tr_count("admin-count-results", 1u64), "1 result");
        assert_eq!(tr_count("admin-count-results", 2u64), "2 results");
        assert_eq!(
            tr_count("admin-count-orders", u64::MAX),
            "18446744073709551615 orders"
        );
        assert_eq!(yes_no(true), "Yes");
        assert_eq!(yes_no(false), "No");
    }
}
