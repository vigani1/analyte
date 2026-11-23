//! Says how a machine is connected, how its link stands, and how its messages were read and delivered.
use super::{Line, Moment, amount, delivery_look};
use crate::admin::server::Connection;
use crate::machine::link::status::{Light, Refusal};
use analyte_model::SampleId;
use analyte_store::{DeliveryState, RecentOutcome};

/// How a machine is attached, in one or two words.
pub(in crate::admin) const fn connection_kind(connection: &Connection) -> &'static str {
    match connection {
        Connection::Serial(_) => "Serial",
        Connection::UsbAdapter(_) => "USB adapter",
        Connection::Listening(_) | Connection::Dialing => "Network",
        Connection::Folder(_) => "Watched folder",
    }
}

/// Where a machine is attached, after its kind: "COM3", "serial A10K3XYZ",
/// "port 5000" or a folder's name; none when the settings name no plain port
/// or Analyte connects out, whose address stays off the pages.
pub(in crate::admin) fn connection_detail(connection: &Connection) -> Option<String> {
    match connection {
        Connection::Serial(port) => Some(port.clone()),
        Connection::UsbAdapter(serial) => Some(format!("serial {serial}")),
        Connection::Listening(port) => port.map(|port| format!("port {port}")),
        Connection::Dialing => None,
        Connection::Folder(folder) => Some(folder.clone()),
    }
}

/// What every page and command says about a record-only machine (ADR 0034).
pub(crate) const RECORDING_ONLY: &str = "Recording only — nothing is sent to a lab app";

/// A record-only machine's counts since start, in words (ADR 0034): what it
/// kept and how many exchanges failed; every byte of both is in the store.
pub(crate) fn recording_counts(tally: crate::machine::link::status::Tally) -> String {
    let count = |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    format!(
        "{} kept, {} since the start; export the recordings with analyte captures export",
        count(tally.messages, "message", "messages"),
        count(tally.failures, "failed exchange", "failed exchanges"),
    )
}

/// What a page says about a listening machine without `allowed_peers` (ADR 0026).
pub(in crate::admin) const OPEN_TO_ANY: &str = "Accepts connections from any device";

/// What to do about [`OPEN_TO_ANY`].
pub(in crate::admin) const OPEN_TO_ANY_ADVICE: &str = concat!(
    "Any device on the lab network that reaches this port can send results as this machine. ",
    "Edit its setup and choose “Only accept this device” after the connection test."
);

/// What to check after a refused connection (ADR 0026).
pub(in crate::admin) const REFUSED_ADVICE: &str = concat!(
    "Only the machine should connect to this port. If this is the machine itself, its ",
    "address changed: add the new address in its setup. If not, find that device."
);

/// Which devices a listening machine accepts, for its page: the allowed
/// addresses, or that any device is accepted; none for other links.
pub(in crate::admin) fn accepts(
    connection: &Connection,
    allowed: Option<&analyte_config::AllowedPeers>,
) -> Option<String> {
    match (connection, allowed) {
        (Connection::Listening(_), Some(allowed)) => Some(format!("Only {allowed}")),
        (Connection::Listening(_), None) => Some("Any device".into()),
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
        format!(
            "Refused {} connections from {}, the last at {at}",
            refusal.count, refusal.peer
        )
    } else {
        format!("Refused a connection from {} at {at}", refusal.peer)
    }
}

/// One accepted difference from the standard in plain words for a lab manager:
/// what the machine does and what Analyte does about it. The exact rule stays on
/// hover and in the declaration under "Details for support".
pub(in crate::admin) const fn departure(rule: &analyte_config::MachineDeparture) -> &'static str {
    use analyte_config::MachineDeparture as D;
    match rule {
        D::Hl7AbsentObr25MeansFinal(_) => concat!(
            "Leaves a report's status empty when the report is final; ",
            "Analyte reads only an empty report status as final.",
        ),
        D::Hl7AbsentObx11MeansFinal(_) => concat!(
            "Leaves a result's status empty when the result is final; ",
            "Analyte reads only an empty result status as final.",
        ),
        D::AstmAbsentH12MeansProduction(_) => {
            "Empty H.12 in a documented upload is treated as production."
        }
        D::AstmRepeatEnqAfterLostAck(_) => {
            "One repeated ENQ before any frame can recover a lost establishment ACK."
        }
        D::AstmPackedRecordsInFrame(_) => concat!(
            "Puts several records in one frame; Analyte reads each record after ",
            "the frame's check passes.",
        ),
        D::Hl7RecordDisplayQuery(_) => "Answer record-format HL7 barcode queries",
        D::Hl7CategorizedOruResults(_) => {
            "Reads combined patient values and divides control uploads into their original rows."
        }
        D::Hl7Msh16MessageCategory(_) => {
            "Puts the upload category in MSH-16. Analyte reads that category on result uploads."
        }
        D::Hl7Utf8InV231(_) => concat!(
            "Marks its text as UTF-8, a name its HL7 version did not have yet; ",
            "Analyte reads the text as UTF-8.",
        ),
        D::Hl7CrLfTerminators(_) => {
            "Ends each message line with CR LF instead of CR; Analyte reads the lines that way."
        }
        D::Hl7LfTerminators(_) => {
            "Ends each message line with LF instead of CR; Analyte reads the lines that way."
        }
        D::Hl7PreserveUnsupportedEscapes(_) => concat!(
            "Uses escape codes Analyte does not translate; Analyte keeps them as written ",
            "and never uses them to identify a sample or result.",
        ),
        D::AstmFrameLimit(_) => concat!(
            "Sends frames of a size other than the standard's; Analyte accepts frames ",
            "up to the limit in this machine's settings.",
        ),
        D::AstmReceiveTimeout(_) => concat!(
            "While the machine sends, Analyte waits for each part as long as this ",
            "machine's settings say, not the standard's default.",
        ),
        D::AstmSendTimeout(_) => concat!(
            "While Analyte sends, it waits for the machine's answers as long as this ",
            "machine's settings say, not the standard's default.",
        ),
        D::AstmHostContentionPriority(_) => {
            "When both sides start at the same moment, Analyte goes first instead of the machine."
        }
        D::AstmLowercaseChecksumHex(_) => concat!(
            "Writes check digits in lowercase; Analyte accepts them only when the ",
            "check is correct.",
        ),
    }
}

/// What a machine's connection does that people may not expect; only a USB
/// adapter has something to say.
pub(in crate::admin) const fn connection_note(connection: &Connection) -> Option<&'static str> {
    match connection {
        Connection::UsbAdapter(_) => Some(
            "Found by its USB serial number, so it keeps working if Windows gives the adapter another port name.",
        ),
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
            "last data "
        } else {
            "Last data "
        })
        .time(at),
        None => Line::text(if model.is_some() {
            "nothing received yet"
        } else {
            "Nothing received yet"
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
        parts.push("No machine is running".to_owned());
    } else {
        parts.push(format!(
            "{} of {} open",
            counts.open,
            amount(counts.running, "link", "links")
        ));
    }
    for (count, one, many) in [
        (counts.problems, "problem", "problems"),
        (
            counts.waiting,
            "waiting for the machine",
            "waiting for the machine",
        ),
        (counts.not_set_up, "not set up yet", "not set up yet"),
        (
            counts.saved,
            "starts after a restart",
            "start after a restart",
        ),
        (
            counts.removed,
            "removed, stops at the restart",
            "removed, stop at the restart",
        ),
    ] {
        if count > 0 {
            parts.push(amount(count, one, many));
        }
    }
    parts.join(" · ")
}

/// How a stored message was read, by its latest reading: "Read · 3 results",
/// "Could not be read", or "Not read yet" when it never was.
pub(in crate::admin) fn reading(outcome: Option<&RecentOutcome>) -> String {
    match outcome {
        None => "Not read yet".into(),
        Some(RecentOutcome::Failed) => "Could not be read".into(),
        Some(RecentOutcome::Results { results: 0, .. }) => "Read · no results".into(),
        Some(RecentOutcome::Results { results, .. }) => format!(
            "Read · {}",
            amount(
                u64::try_from(*results).unwrap_or(u64::MAX),
                "result",
                "results"
            )
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
        words.push_str(&format!(" · {done} of {total} delivered"));
    } else if worst == DeliveryState::Done && total > 1 {
        words = format!("{total} delivered");
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
        DeliveryState::Done => 0,
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
        (Connection::Serial(port), Light::Live) => format!("Port {port} open since {since}"),
        (Connection::Serial(port), Light::Waiting | Light::Off) => {
            format!("Port {port} not open since {since}")
        }
        (Connection::UsbAdapter(serial), Light::Live) => {
            format!("Port of USB adapter {serial} open since {since}")
        }
        (Connection::UsbAdapter(serial), Light::Waiting | Light::Off) => {
            format!("Port of USB adapter {serial} not open since {since}")
        }
        (Connection::Listening(port), Light::Live) => {
            format!("Machine connected{} since {since}", on_port(*port))
        }
        (Connection::Listening(port), Light::Waiting) => {
            format!(
                "Listening{}, nothing connected since {since}",
                on_port(*port)
            )
        }
        (Connection::Listening(port), Light::Off) => {
            format!("Not listening{} since {since}", on_port(*port))
        }
        (Connection::Dialing, Light::Live) => format!("Connected to {machine} since {since}"),
        (Connection::Dialing, Light::Waiting | Light::Off) => {
            format!("No connection to {machine} since {since}")
        }
        (Connection::Folder(folder), Light::Live) => format!("Watching {folder}"),
        (Connection::Folder(folder), Light::Waiting | Light::Off) => {
            format!("Not watching {folder} since {since}")
        }
    }
}

/// " on port 5000", or nothing when the site file names no plain port.
fn on_port(port: Option<u16>) -> String {
    port.map_or_else(String::new, |port| format!(" on port {port}"))
}

/// What to check when a machine's link is not open: what Analyte cannot see.
/// A serial port that will not open is a problem on this PC (the adapter or
/// cable unplugged from it, a wrong port name, or another program holding the
/// port); the cable to the machine and its host settings matter once the port
/// is open and nothing arrives, which the listen test says.
pub(in crate::admin) fn link_advice(connection: &Connection) -> String {
    match connection {
        Connection::Serial(port) => format!(
            "Check that the USB adapter or cable is plugged into this PC, that {port} is the right port, and that no other program is using it."
        ),
        Connection::UsbAdapter(serial) => format!(
            "Check that the USB adapter with serial {serial} is plugged into this PC and that no other program is using its port."
        ),
        Connection::Listening(_) | Connection::Dialing => {
            "Check the network cable and the machine's host settings.".into()
        }
        Connection::Folder(_) => {
            "Check that the folder exists and that Analyte's service account may read it.".into()
        }
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
