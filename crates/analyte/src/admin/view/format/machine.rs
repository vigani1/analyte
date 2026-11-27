//! Says how a machine is connected, how its link stands, and how its messages were read and delivered.
use super::{Line, Moment, delivery_look, nonzero_count};
use crate::admin::server::Connection;
use crate::machine::link::status::{Light, Refusal};
use analyte_i18n::{count as tr_count, message as tr_args, text as tr};
use analyte_model::SampleId;
use analyte_store::{DeliveryState, RecentOutcome};

/// How a machine is attached, in one or two words.
pub(in crate::admin) fn connection_kind(connection: &Connection) -> &'static str {
    match connection {
        Connection::Serial(_) => tr("admin-serial-2"),
        Connection::UsbAdapter(_) => tr("admin-usb-adapter"),
        Connection::Listening(_) | Connection::Dialing => tr("admin-network"),
        Connection::Folder(_) => tr("admin-watched-folder"),
    }
}

/// Where a machine is attached, after its kind: "COM3", "serial A10K3XYZ",
/// "port 5000" or a folder's name; none when the settings name no plain port
/// or Analyte connects out, whose address stays off the pages.
pub(in crate::admin) fn connection_detail(connection: &Connection) -> Option<String> {
    match connection {
        Connection::Serial(port) => Some(port.clone()),
        Connection::UsbAdapter(serial) => Some(tr_args(
            "admin-serial",
            [("serial", (serial).to_string().into())],
        )),
        Connection::Listening(port) => {
            port.map(|port| tr_args("admin-port", [("port", (port).to_string().into())]))
        }
        Connection::Dialing => None,
        Connection::Folder(folder) => Some(folder.clone()),
    }
}

/// What every page and command says about a record-only machine (ADR 0034).
pub(crate) fn recording_only() -> &'static str {
    tr("admin-recording-only")
}

/// A record-only machine's counts since start, in words (ADR 0034): what it
/// kept and how many exchanges failed; every byte of both is in the store.
pub(crate) fn recording_counts(tally: crate::machine::link::status::Tally) -> String {
    tr_args(
        "admin-kept-since-the-start-export-the-recordings-with-analyte",
        [
            (
                "messages",
                tr_count("admin-count-recorded-messages", tally.messages).into(),
            ),
            (
                "exchanges",
                tr_count("admin-count-failed-exchanges", tally.failures).into(),
            ),
        ],
    )
}

/// What a page says about a listening machine without `allowed_peers` (ADR 0026).
pub(in crate::admin) fn open_to_any() -> &'static str {
    tr("admin-open-to-any")
}

/// [`open_to_any`] in the middle of a sentence, as each language writes it there.
pub(in crate::admin) fn open_to_any_in_line() -> &'static str {
    tr("admin-open-to-any-in-line")
}

/// What to do about [`OPEN_TO_ANY`].
pub(in crate::admin) fn open_to_any_advice() -> &'static str {
    tr("admin-open-to-any-advice")
}

/// What to check after a refused connection (ADR 0026).
pub(in crate::admin) fn refused_advice() -> &'static str {
    tr("admin-refused-advice")
}

/// Which devices a listening machine accepts, for its page: the allowed
/// addresses, or that any device is accepted; none for other links.
pub(in crate::admin) fn accepts(
    connection: &Connection,
    allowed: Option<&analyte_config::AllowedPeers>,
) -> Option<String> {
    match (connection, allowed) {
        (Connection::Listening(_), Some(allowed)) => Some(only_devices(allowed)),
        (Connection::Listening(_), None) => Some(any_device().into()),
        (
            Connection::Serial(_)
            | Connection::UsbAdapter(_)
            | Connection::Dialing
            | Connection::Folder(_),
            _,
        ) => None,
    }
}

/// One refused device in words: "Refused a connection from 192.168.1.77 at
/// 14:32", or with its count when it tried more than once.
pub(in crate::admin) fn refused(refusal: &Refusal, at: &str) -> String {
    if refusal.count > 1 {
        tr_args(
            "admin-refused-connections-from-the-last-at",
            [
                ("count", (refusal.count).to_string().into()),
                ("peer", (refusal.peer).to_string().into()),
                ("at", (at).to_string().into()),
            ],
        )
    } else {
        tr_args(
            "admin-refused-a-connection-from-at",
            [
                ("peer", (refusal.peer).to_string().into()),
                ("at", (at).to_string().into()),
            ],
        )
    }
}

/// One accepted difference from the standard in plain words for a lab manager:
/// what the machine does and what Analyte does about it. The exact rule stays on
/// hover and in the declaration under "Details for support".
pub(in crate::admin) fn departure(rule: &analyte_config::MachineDeparture) -> &'static str {
    use analyte_config::MachineDeparture as D;
    match rule {
        D::Hl7AbsentObr25MeansFinal(_) => {
            tr("admin-leaves-a-report-s-status-empty-when-the-report")
        }
        D::Hl7AbsentObx11MeansFinal(_) => {
            tr("admin-leaves-a-result-s-status-empty-when-the-result")
        }
        D::AstmAbsentH12MeansProduction(_) => {
            tr("admin-empty-h-12-in-a-documented-upload-is-treated")
        }
        D::AstmRepeatEnqAfterLostAck(_) => {
            tr("admin-one-repeated-enq-before-any-frame-can-recover-a")
        }
        D::AstmPackedRecordsInFrame(_) => {
            tr("admin-puts-several-records-in-one-frame-analyte-reads-each")
        }
        D::Hl7RecordDisplayQuery(_) => tr("admin-answer-record-format-hl7-barcode-queries"),
        D::Hl7CategorizedOruResults(_) => {
            tr("admin-reads-combined-patient-values-and-divides-control-uploads-into")
        }
        D::Hl7Msh16MessageCategory(_) => {
            tr("admin-puts-the-upload-category-in-msh-16-analyte-reads")
        }
        D::Hl7Utf8InV231(_) => tr("admin-marks-its-text-as-utf-8-a-name-its"),
        D::Hl7CrLfTerminators(_) => tr("admin-ends-each-message-line-with-cr-lf-instead-of"),
        D::Hl7LfTerminators(_) => tr("admin-ends-each-message-line-with-lf-instead-of-cr"),
        D::Hl7PreserveUnsupportedEscapes(_) => {
            tr("admin-uses-escape-codes-analyte-does-not-translate-analyte-keeps")
        }
        D::AstmFrameLimit(_) => tr("admin-sends-frames-of-a-size-other-than-the-standard"),
        D::AstmReceiveTimeout(_) => tr("admin-while-the-machine-sends-analyte-waits-for-each-part"),
        D::AstmSendTimeout(_) => tr("admin-while-analyte-sends-it-waits-for-the-machine-s"),
        D::AstmHostContentionPriority(_) => {
            tr("admin-when-both-sides-start-at-the-same-moment-analyte")
        }
        D::AstmLowercaseChecksumHex(_) => {
            tr("admin-writes-check-digits-in-lowercase-analyte-accepts-them-only")
        }
    }
}

/// What a machine's connection does that people may not expect; only a USB
/// adapter has something to say.
pub(in crate::admin) fn connection_note(connection: &Connection) -> Option<&'static str> {
    match connection {
        Connection::UsbAdapter(_) => Some(tr("admin-found-by-its-usb-serial-number-so-it-keeps")),
        Connection::Serial(_)
        | Connection::Listening(_)
        | Connection::Dialing
        | Connection::Folder(_) => None,
    }
}

/// The line under a machine's name: its model and when data last came, such
/// as "XC-200 · last data 10:41" or "Nothing received yet".
pub(in crate::admin) fn machine_lede(model: Option<&str>, last: Option<Moment>) -> Line {
    let data = match last {
        Some(at) => Line::text(if model.is_some() {
            tr("admin-last-data-2")
        } else {
            tr("admin-last-data-3")
        })
        .time(at),
        None => Line::text(if model.is_some() {
            tr("admin-nothing-received-yet-2")
        } else {
            nothing_received_yet()
        }),
    };
    match model {
        Some(model) => Line::text(model).then(data),
        None => data,
    }
}

/// Control characters by their standard names in angle brackets, in the
/// order given: "<ENQ> <STX> <CR> <ETX>". They frame a message and say
/// nothing of what it holds; any other byte is left out.
pub(in crate::admin) fn control_names(bytes: &[u8]) -> String {
    const C0: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
        "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB",
        "ESC", "FS", "GS", "RS", "US",
    ];
    bytes
        .iter()
        .filter_map(|&byte| match byte {
            0x7f => Some("DEL"),
            byte => C0.get(usize::from(byte)).copied(),
        })
        .map(|name| format!("<{name}>"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// How the set-up machines stand, as the Machines page counts them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::admin) struct MachineCounts {
    /// Running machines whose link is open: connected, port open or watching.
    pub(in crate::admin) open: u64,
    /// Machines Analyte runs now.
    pub(in crate::admin) running: u64,
    /// Running machines whose link is off, or which Analyte stopped working with.
    pub(in crate::admin) problems: u64,
    /// Running machines Analyte listens for that have not connected.
    pub(in crate::admin) waiting: u64,
    /// Listed in the settings without a model chosen.
    pub(in crate::admin) not_set_up: u64,
    /// Set up in the settings, running after the next restart.
    pub(in crate::admin) saved: u64,
    /// Taken out of the settings, still running until the restart.
    pub(in crate::admin) removed: u64,
}

/// What the Machines page says under its title: "3 of 5 links open · 1 problem
/// · 1 waiting for the machine"; a part with nothing to count is left out.
pub(in crate::admin) fn machines_summary(counts: &MachineCounts) -> String {
    let mut parts = Vec::new();
    if counts.running == 0 {
        parts.push(no_machine_running().to_owned());
    } else {
        parts.push(tr_args(
            "admin-of-open",
            [
                ("open", (counts.open).to_string().into()),
                (
                    "links",
                    (tr_count("admin-count-links", counts.running))
                        .to_string()
                        .into(),
                ),
            ],
        ));
    }
    parts.extend(
        [
            nonzero_count("admin-count-machine-problems", counts.problems),
            nonzero_count("admin-count-machines-waiting", counts.waiting),
            nonzero_count("admin-count-machines-awaiting-setup", counts.not_set_up),
            nonzero_count("admin-count-machines-start-on-restart", counts.saved),
            nonzero_count("admin-count-machines-stop-on-restart", counts.removed),
        ]
        .into_iter()
        .flatten(),
    );
    parts.join(" · ")
}

/// How a stored message was read, by its latest reading: "Read · 3 results",
/// "Could not be read", or "Not read yet" when it never was.
pub(in crate::admin) fn reading(outcome: Option<&RecentOutcome>) -> String {
    match outcome {
        None => tr("admin-not-read-yet").into(),
        Some(RecentOutcome::Failed) => tr("admin-could-not-be-read").into(),
        Some(RecentOutcome::Results { results: 0, .. }) => tr("admin-read-no-results").into(),
        Some(RecentOutcome::Results { results, .. }) => tr_args(
            "admin-read-results",
            [(
                "results",
                tr_count(
                    "admin-count-results",
                    u64::try_from(*results).unwrap_or(u64::MAX),
                )
                .into(),
            )],
        ),
    }
}

/// A message's sample IDs exactly as stored: the first, with how many more
/// follow ("261119-044 +1"), and all of them for the hover; none without any.
pub(in crate::admin) fn samples(samples: &[SampleId]) -> Option<(String, String)> {
    let (first, rest) = samples.split_first()?;
    let shown = if rest.is_empty() {
        first.as_str().to_owned()
    } else {
        format!("{} +{}", first.as_str(), rest.len())
    };
    let all = samples
        .iter()
        .map(SampleId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    Some((shown, all))
}

/// How a message's deliveries stand, by the one that most needs someone:
/// its state in words and, when some differ, how many were delivered ("Waiting
/// to be sent · 2 of 3 delivered"); the count per state for the hover. None
/// when nothing was queued for it.
pub(in crate::admin) fn deliveries(
    deliveries: &[(DeliveryState, u64)],
) -> Option<(DeliveryState, String, String)> {
    let worst = deliveries
        .iter()
        .map(|&(state, _)| state)
        .max_by_key(|&state| delivery_rank(state))?;
    let total = deliveries.iter().map(|&(_, count)| count).sum::<u64>();
    let done = deliveries
        .iter()
        .filter(|&&(state, _)| state == DeliveryState::Done)
        .map(|&(_, count)| count)
        .sum::<u64>();
    let mut words = delivery_look(worst).word.to_owned();
    if worst != DeliveryState::Done && done > 0 {
        words.push_str(&tr_args(
            "admin-of-delivered",
            [
                ("done", (done).to_string().into()),
                ("total", (total).to_string().into()),
            ],
        ));
    } else if worst == DeliveryState::Done && total > 1 {
        words = tr_args("admin-delivered-2", [("total", (total).to_string().into())]);
    }
    let exact = deliveries
        .iter()
        .map(|(state, count)| format!("{state} {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    Some((worst, words, exact))
}

/// How much a delivery state needs someone: delivered least, a conflict most.
const fn delivery_rank(state: DeliveryState) -> u8 {
    match state {
        DeliveryState::Done | DeliveryState::Declined => 0,
        DeliveryState::InFlight => 1,
        DeliveryState::Pending => 2,
        DeliveryState::Paused => 3,
        DeliveryState::Parked => 4,
        DeliveryState::Conflict => 5,
    }
}

/// What Analyte observes about a machine's link, with the time it last changed:
/// "Port COM3 not open since 10:41", "Listening on port 5000, nothing connected
/// since 10:41", "No connection to cobas-e411 since 10:41", "Watching results".
pub(in crate::admin) fn link_sentence(
    connection: &Connection,
    light: Light,
    since: &str,
    machine: &str,
) -> String {
    match (connection, light) {
        (Connection::Serial(port), Light::Live) => tr_args(
            "admin-port-open-since",
            [
                ("port", (port).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Serial(port), Light::Waiting | Light::Off) => tr_args(
            "admin-port-not-open-since",
            [
                ("port", (port).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::UsbAdapter(serial), Light::Live) => tr_args(
            "admin-port-of-usb-adapter-open-since",
            [
                ("serial", (serial).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::UsbAdapter(serial), Light::Waiting | Light::Off) => tr_args(
            "admin-port-of-usb-adapter-not-open-since",
            [
                ("serial", (serial).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Listening(port), Light::Live) => tr_args(
            "admin-machine-connected-since",
            [
                ("port", (on_port(*port)).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Listening(port), Light::Waiting) => tr_args(
            "admin-listening-nothing-connected-since",
            [
                ("port", (on_port(*port)).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Listening(port), Light::Off) => tr_args(
            "admin-not-listening-since",
            [
                ("port", (on_port(*port)).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Dialing, Light::Live) => tr_args(
            "admin-connected-to-since",
            [
                ("machine", (machine).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Dialing, Light::Waiting | Light::Off) => tr_args(
            "admin-no-connection-to-since",
            [
                ("machine", (machine).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
        (Connection::Folder(folder), Light::Live) => {
            tr_args("admin-watching", [("folder", (folder).to_string().into())])
        }
        (Connection::Folder(folder), Light::Waiting | Light::Off) => tr_args(
            "admin-not-watching-since",
            [
                ("folder", (folder).to_string().into()),
                ("since", (since).to_string().into()),
            ],
        ),
    }
}

/// " on port 5000", or nothing when the site file names no plain port.
fn on_port(port: Option<u16>) -> String {
    port.map_or_else(String::new, |port| {
        tr_args("admin-on-port", [("port", (port).to_string().into())])
    })
}

/// What to check when a machine's link is not open: what Analyte cannot see.
/// A serial port that will not open is a problem on this PC (the adapter or
/// cable unplugged from it, a wrong port name, or another program holding the
/// port); the cable to the machine and its host settings matter once the port
/// is open and nothing arrives, which the listen test says.
pub(in crate::admin) fn link_advice(connection: &Connection) -> String {
    match connection {
        Connection::Serial(port) => tr_args(
            "admin-check-that-the-usb-adapter-or-cable-is-plugged",
            [("port", (port).to_string().into())],
        ),
        Connection::UsbAdapter(serial) => tr_args(
            "admin-check-that-the-usb-adapter-with-serial-is-plugged",
            [("serial", (serial).to_string().into())],
        ),
        Connection::Listening(_) | Connection::Dialing => {
            tr("admin-check-the-network-cable-and-the-machine-s-host").into()
        }
        Connection::Folder(_) => tr("admin-check-that-the-folder-exists-and-that-analyte-s").into(),
    }
}

/// Which reader read a message, in words: "astm, version 2".
pub(in crate::admin) fn driver(driver: &analyte_model::NamespacedId) -> String {
    format!(
        "{}, version {}",
        driver.namespace.as_str(),
        driver.value.as_str()
    )
}

/// The connection accepts devices without an address restriction.
pub(in crate::admin) fn any_device() -> &'static str {
    tr("admin-any-device")
}

/// The page's wording for the addresses a listening port accepts.
pub(in crate::admin) fn only_devices(addresses: impl std::fmt::Display) -> String {
    tr_args(
        "admin-only",
        [("addresses", (addresses).to_string().into())],
    )
}

/// The state before a machine has been configured.
pub(in crate::admin) fn not_set_up() -> &'static str {
    tr("admin-not-set-up")
}

/// The state of a saved machine the engine has yet to start.
pub(in crate::admin) fn starts_after_restart() -> &'static str {
    tr("admin-starts-after-restart")
}

/// The state of a removed machine the engine still runs.
pub(in crate::admin) fn stops_at_restart() -> &'static str {
    tr("admin-stops-at-restart")
}

/// Heads the warning when machine settings cannot be read.
pub(in crate::admin) fn settings_unreadable() -> &'static str {
    tr("admin-settings-unreadable")
}

/// Names the action to add a machine in Setup.
pub(in crate::admin) fn add_machine() -> &'static str {
    tr("admin-add-machine")
}

/// The machine model has not been chosen.
pub(in crate::admin) fn not_chosen() -> &'static str {
    tr("admin-not-chosen")
}

/// The saved machine has yet to run.
pub(in crate::admin) fn not_running() -> &'static str {
    tr("admin-not-running")
}

/// Heads the connection state inside Analyte.
pub(in crate::admin) fn in_analyte() -> &'static str {
    tr("admin-in-analyte")
}

/// Labels the kind of connection.
pub(in crate::admin) fn connected_by() -> &'static str {
    tr("admin-connected-by")
}

/// Labels the devices a machine accepts.
pub(in crate::admin) fn accepts_from() -> &'static str {
    tr("admin-accepts-from")
}

/// Labels the serial cable settings.
pub(in crate::admin) fn cable_settings() -> &'static str {
    tr("admin-cable-settings")
}

/// Labels choices from the analyzer model.
pub(in crate::admin) fn analyzer_settings() -> &'static str {
    tr("admin-analyzer-settings")
}

/// Labels when data last arrived from a machine.
pub(in crate::admin) fn last_data() -> &'static str {
    tr("admin-last-data")
}

/// Labels the lab app that receives this machine’s results.
pub(in crate::admin) fn sends_results_to() -> &'static str {
    tr("admin-sends-results-to")
}

/// Labels the model of a machine.
pub(in crate::admin) fn model() -> &'static str {
    tr("admin-model")
}

/// Labels the configured connection.
pub(in crate::admin) fn connection() -> &'static str {
    tr("admin-connection")
}

/// Labels the machine associated with the settings or data.
pub(in crate::admin) fn machine() -> &'static str {
    tr("admin-machine")
}

/// Labels the link to a machine’s details.
pub(in crate::admin) fn open_machine() -> &'static str {
    tr("admin-open-machine")
}

/// Says that the machine has sent no data in the activity being shown.
pub(in crate::admin) fn nothing_received_yet() -> &'static str {
    tr("admin-nothing-received-yet")
}

/// Reports that the engine has no running machines.
pub(in crate::admin) fn no_machine_running() -> &'static str {
    tr("admin-no-machine-running")
}

/// Reports that none of the machines have been set up.
pub(in crate::admin) fn no_machine_set_up() -> &'static str {
    tr("admin-no-machine-set-up")
}

#[cfg(test)]
mod tests {
    use super::super::{countdown, moment};
    use super::*;
    use jiff::Timestamp;

    // Owner request 2026-11-19 (admin restyle): each link type says only what
    // Analyte observed, with the time of the last change.
    #[test]
    fn link_sentences_say_only_what_was_observed() {
        let since = "10:41";
        for (connection, light, expected) in [
            (
                Connection::Serial("COM3".into()),
                Light::Off,
                "Port COM3 not open since 10:41",
            ),
            (
                Connection::Serial("COM3".into()),
                Light::Live,
                "Port COM3 open since 10:41",
            ),
            (
                Connection::UsbAdapter("A9X".into()),
                Light::Off,
                "Port of USB adapter A9X not open since 10:41",
            ),
            (
                Connection::Listening(Some(5000)),
                Light::Waiting,
                "Listening on port 5000, nothing connected since 10:41",
            ),
            (
                Connection::Listening(Some(5000)),
                Light::Live,
                "Machine connected on port 5000 since 10:41",
            ),
            (
                Connection::Listening(None),
                Light::Off,
                "Not listening since 10:41",
            ),
            (
                Connection::Dialing,
                Light::Off,
                "No connection to lumi since 10:41",
            ),
            (
                Connection::Dialing,
                Light::Live,
                "Connected to lumi since 10:41",
            ),
            (
                Connection::Folder("results".into()),
                Light::Live,
                "Watching results",
            ),
            (
                Connection::Folder("results".into()),
                Light::Off,
                "Not watching results since 10:41",
            ),
        ] {
            let sentence = link_sentence(&connection, light, since, "lumi");
            assert_eq!(sentence, expected);
            for unseen in [
                "cable not connected",
                "switched on",
                "onnected to the machine",
            ] {
                assert!(!sentence.contains(unseen), "{sentence}");
            }
        }
        // A port that will not open is a problem on this PC, not at the machine.
        assert_eq!(
            link_advice(&Connection::Serial("COM3".into())),
            "Check that the USB adapter or cable is plugged into this PC, that COM3 is the right port, and that no other program is using it."
        );
        assert_eq!(
            link_advice(&Connection::UsbAdapter("FT4XQ2B9".into())),
            "Check that the USB adapter with serial FT4XQ2B9 is plugged into this PC and that no other program is using its port."
        );
        assert_eq!(
            link_advice(&Connection::Dialing),
            "Check the network cable and the machine's host settings."
        );
        assert_eq!(
            connection_kind(&Connection::UsbAdapter("A9X".into())),
            "USB adapter"
        );
        assert_eq!(connection_kind(&Connection::Dialing), "Network");
    }

    // Owner decisions 2026-11-20 (admin restyle b2, Machines mockup): the line
    // under the Machines title counts open links of all running machines, then
    // only the states that have a machine in them.
    #[test]
    fn machines_summary_counts_only_what_there_is() {
        assert_eq!(
            machines_summary(&MachineCounts::default()),
            "No machine is running"
        );
        assert_eq!(
            machines_summary(&MachineCounts {
                open: 3,
                running: 5,
                problems: 1,
                waiting: 1,
                ..MachineCounts::default()
            }),
            "3 of 5 links open · 1 problem · 1 waiting for the machine"
        );
        assert_eq!(
            machines_summary(&MachineCounts {
                open: 1,
                running: 1,
                problems: 2,
                waiting: 2,
                not_set_up: 2,
                saved: 2,
                removed: 2,
            }),
            concat!(
                "1 of 1 link open · 2 problems · 2 waiting for the machine · 2 not set up yet · ",
                "2 start after a restart · 2 removed, stop at the restart"
            )
        );
        assert_eq!(
            machines_summary(&MachineCounts {
                not_set_up: 1,
                ..MachineCounts::default()
            }),
            "No machine is running · 1 not set up yet"
        );
    }

    // Owner decisions 2026-11-20 (admin restyle b2): a stored message reads by
    // its latest reading, its sample IDs exactly as stored (leading zeros and
    // case kept), and its deliveries by the one that most needs someone.
    #[test]
    fn messages_read_by_reading_samples_and_deliveries() -> Result<(), analyte_model::TextError> {
        assert_eq!(reading(None), "Not read yet");
        assert_eq!(reading(Some(&RecentOutcome::Failed)), "Could not be read");
        let results = |results| RecentOutcome::Results {
            results,
            samples: Vec::new(),
        };
        assert_eq!(reading(Some(&results(0))), "Read · no results");
        assert_eq!(reading(Some(&results(1))), "Read · 1 result");
        assert_eq!(reading(Some(&results(6))), "Read · 6 results");
        assert_eq!(samples(&[]), None);
        let one = [SampleId::new("000123")?];
        assert_eq!(samples(&one), Some(("000123".into(), "000123".into())));
        let three = [
            SampleId::new("S-0042")?,
            SampleId::new("s-0042")?,
            SampleId::new("0007")?,
        ];
        assert_eq!(
            samples(&three),
            Some(("S-0042 +2".into(), "S-0042, s-0042, 0007".into()))
        );
        assert_eq!(deliveries(&[]), None);
        assert_eq!(
            deliveries(&[(DeliveryState::Done, 1)]),
            Some((DeliveryState::Done, "Delivered".into(), "Done 1".into()))
        );
        assert_eq!(
            deliveries(&[(DeliveryState::Done, 3)]),
            Some((DeliveryState::Done, "3 delivered".into(), "Done 3".into()))
        );
        assert_eq!(
            deliveries(&[(DeliveryState::Pending, 2)]),
            Some((
                DeliveryState::Pending,
                "Waiting to be sent".into(),
                "Pending 2".into()
            ))
        );
        assert_eq!(
            deliveries(&[
                (DeliveryState::Done, 2),
                (DeliveryState::Conflict, 1),
                (DeliveryState::InFlight, 1),
            ]),
            Some((
                DeliveryState::Conflict,
                "Lab app reports a conflict · 2 of 4 delivered".into(),
                "Done 2, Conflict 1, InFlight 1".into()
            ))
        );
        for (worse, better) in [
            (DeliveryState::Conflict, DeliveryState::Parked),
            (DeliveryState::Parked, DeliveryState::Paused),
            (DeliveryState::Paused, DeliveryState::Pending),
            (DeliveryState::Pending, DeliveryState::InFlight),
            (DeliveryState::InFlight, DeliveryState::Done),
        ] {
            assert_eq!(
                deliveries(&[(better, 1), (worse, 1)]).map(|(state, ..)| state),
                Some(worse)
            );
        }
        Ok(())
    }

    // Owner decisions 2026-11-20 (admin restyle b2) and safety rule 10: how a
    // machine is attached names its port, adapter, listening port or folder,
    // never an address Analyte connects out to; only a USB adapter has a note.
    #[test]
    fn connections_name_their_port_never_an_address() {
        for (connection, detail) in [
            (Connection::Serial("COM3".into()), Some("COM3")),
            (
                Connection::UsbAdapter("A10K3XYZ".into()),
                Some("serial A10K3XYZ"),
            ),
            (Connection::Listening(Some(5600)), Some("port 5600")),
            (Connection::Listening(None), None),
            (Connection::Dialing, None),
            (Connection::Folder("results".into()), Some("results")),
        ] {
            assert_eq!(connection_detail(&connection).as_deref(), detail);
            assert_eq!(
                connection_note(&connection).is_some(),
                matches!(connection, Connection::UsbAdapter(_)),
                "{connection:?}"
            );
        }
    }

    // Owner decisions 2026-11-20 (admin restyle b2, MachineDetail mockup): the
    // line under a machine's name gives its model and when data last came.
    #[test]
    fn machine_lede_names_the_model_and_last_data() {
        let now = Timestamp::from_second(1_795_046_077).expect("a valid test time");
        let plain = |line: Line| line.plain();
        assert_eq!(
            plain(machine_lede(Some("XC-200"), None)),
            "XC-200 · nothing received yet"
        );
        assert_eq!(plain(machine_lede(None, None)), "Nothing received yet");
        let last = plain(machine_lede(Some("XC-200"), Some(moment(now, now))));
        assert!(last.starts_with("XC-200 · last data "), "{last}");
        let bare = plain(machine_lede(None, Some(moment(now, now))));
        assert!(bare.starts_with("Last data "), "{bare}");
        assert_eq!(
            driver(&analyte_model::NamespacedId {
                namespace: analyte_model::Identifier::new("astm").expect("a valid name"),
                value: analyte_model::Identifier::new("2").expect("a valid version"),
            }),
            "astm, version 2"
        );
    }

    // Owner decision 2026-11-20 (admin restyle b4): the setup test shows the
    // framing characters by their ASCII names (ANSI X3.4 / ISO 646 control
    // codes), never a printable byte; its countdown reads minutes and seconds.
    #[test]
    fn framing_characters_read_by_name_and_countdowns_by_minutes() {
        assert_eq!(
            control_names(&[0x05, 0x02, 0x0d, 0x03, 0x04, 0x0a]),
            "<ENQ> <STX> <CR> <ETX> <EOT> <LF>"
        );
        assert_eq!(
            control_names(&[0x0b, 0x1c, 0x1e, 0x7f, 0x00]),
            "<VT> <FS> <RS> <DEL> <NUL>"
        );
        assert_eq!(control_names(b"MSH|PID"), "");
        assert_eq!(countdown(41), "0:41");
        assert_eq!(countdown(60), "1:00");
        assert_eq!(countdown(0), "0:00");
    }
}
