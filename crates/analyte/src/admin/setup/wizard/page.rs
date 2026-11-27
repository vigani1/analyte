//! Holds the setup steps' view structs: the frame around every step, each step's form, the live connection test and the save, as display-ready values.
//!
//! Every value comes from the files, the shipped models, the port list or the
//! test. The test shows only what Analyte observed: how many bytes arrived,
//! which framing characters they used and how the stream starts, never the
//! message itself.
use crate::admin::view::parts::{Hover, Notice};
use crate::admin::view::{Tone, format::Moment, frame::Frame, icon::Icon, report::Status};

/// What every step shows around its form: the fixed machine name, the
/// model chosen so far and the four steps with this one marked.
pub(super) struct Wizard {
    pub(super) id: String,
    /// The chosen model's name, beside the machine's fixed name; none before one is chosen.
    pub(super) model: Option<String>,
    pub(super) steps: Vec<Mark>,
    /// "Step 2 of 4".
    pub(super) count: String,
    /// Cancel goes back to Setup; not once the settings are saved.
    pub(super) cancel: bool,
}

/// One step in the stepper: done, current or still to come.
pub(super) struct Mark {
    pub(super) number: &'static str,
    pub(super) label: &'static str,
    pub(super) done: bool,
    pub(super) current: bool,
}

/// The earlier choices a step's form carries as hidden fields; never a secret.
pub(super) type Hidden = Vec<(String, String)>;

/// GET /setup/{id}: step 1, which machine model it is.
#[derive(askama::Template)]
#[template(path = "pages/setup/wizard/model.html")]
pub(super) struct ModelStep {
    pub(super) frame: Frame,
    pub(super) wizard: Wizard,
    /// Step 2, where Next goes.
    pub(super) action: String,
    pub(super) hidden: Hidden,
    pub(super) choices: Vec<Choice>,
    /// Record only is chosen (ADR 0034): nothing is sent to a lab app.
    pub(super) records_only: bool,
}

/// One model to pick, or keeping the current one.
pub(super) struct Choice {
    pub(super) value: String,
    /// Its name, with its folder key on hover.
    pub(super) title: Hover,
    /// How it was checked, or the kept definition's path.
    pub(super) meta: Hover,
    /// Ready, Not ready yet or Cannot be read, with its icon; none for
    /// keeping the current model.
    pub(super) status: Option<(Status, Icon)>,
    pub(super) checked: bool,
    pub(super) disabled: bool,
}

/// GET /setup/{id}/connection: step 2, how it is connected and which lab app gets its results.
#[derive(askama::Template)]
#[template(path = "pages/setup/wizard/connection.html")]
pub(super) struct ConnectionStep {
    pub(super) frame: Frame,
    pub(super) wizard: Wizard,
    /// Step 3, where Next goes, and step 1, where Back goes.
    pub(super) action: String,
    pub(super) back: String,
    pub(super) hidden: Hidden,
    /// How this model connects, in one sentence.
    pub(super) how: &'static str,
    pub(super) inputs: Inputs,
    /// The model's own choices (ADR 0032); empty when it declares none.
    pub(super) choices: Vec<Setting>,
    pub(super) vidas_patient_namespace: Option<String>,
    pub(super) autolyser_poll_seconds: Option<String>,
    pub(super) normal_poll_range: std::ops::RangeInclusive<u32>,
    pub(super) recording_poll_range: std::ops::RangeInclusive<u32>,
    pub(super) lab_apps: Picker,
    /// Record only (ADR 0034): the capture profile's questions, and no lab app.
    pub(super) recording: Option<Recording>,
    /// Record only: the analyzer mode the model's record file asks, if any.
    pub(super) analyzer_mode: Option<Setting>,
    /// A normal connection whose model reconciles through identity recipes:
    /// how repeated results are handled (ADR 0035, 2026-11-26).
    pub(super) repeated_results: Option<Setting>,
}

/// Record only, step 2: what the protocol's capture profile asks the lab.
pub(super) enum Recording {
    /// ASTM and HL7 need only the connection.
    Nothing,
    /// The VIDAS PC's acknowledgment point, `literal` or `alternate`.
    Vidas { acknowledgment: String },
    /// Seconds between next-report requests, and whether reports carry controls.
    Autolyser {
        poll_seconds: String,
        controls: String,
    },
}

/// One choice the model declares, on step 2: its declared values with the
/// chosen one, and the manual's suggestion.
pub(super) struct Setting {
    /// The form field, `choice.<name>`.
    pub(super) name: String,
    /// The element id.
    pub(super) id: String,
    /// The question in plain words.
    pub(super) label: String,
    pub(super) options: Vec<CableOption>,
    /// A value is chosen; without one the list starts at "Choose".
    pub(super) chosen: bool,
    /// Where to read it and what the manual suggests.
    pub(super) hint: String,
    /// The chosen value is not the manual's suggestion.
    pub(super) differs: bool,
}

/// The connection fields the model's transport asks for.
pub(super) enum Inputs {
    /// The detected ports, or why they could not be listed, then a typed name.
    Serial {
        ports: Result<Vec<Port>, String>,
        typed: String,
        placeholder: String,
        /// The cable settings, each from its standard list (ADR 0031).
        cable: Vec<Cable>,
    },
    /// Analyte listens and the machine connects: which of this PC's
    /// addresses to listen on (or why they could not be listed, then a typed
    /// address), and the port.
    Listens {
        hosts: Result<Vec<Host>, String>,
        typed: String,
        port: String,
        /// The devices it accepts, as typed or saved; empty accepts any (ADR 0026).
        peers: String,
    },
    /// Analyte connects to the machine: its IP address and port.
    Connects { host: String, port: String },
    /// A folder Analyte watches.
    Folder(String),
}

/// One serial cable setting on step 2: its standard list with the chosen
/// value, and the machine definition's suggestion.
pub(super) struct Cable {
    /// The form field, such as `baud`.
    pub(super) name: &'static str,
    pub(super) label: &'static str,
    pub(super) options: Vec<CableOption>,
    /// A value is chosen; without one the list starts at "Choose".
    pub(super) chosen: bool,
    /// What the manual suggests, or that it suggests nothing.
    pub(super) hint: String,
    /// The chosen value is not the manual's suggestion.
    pub(super) differs: bool,
}

/// One value of a cable setting's list.
pub(super) struct CableOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) selected: bool,
}

/// One detected serial port.
pub(super) struct Port {
    pub(super) value: String,
    pub(super) name: String,
    /// The adapter and how Analyte remembers the port.
    pub(super) meta: String,
    /// The running machine that holds it, so it cannot be chosen.
    pub(super) used_by: Option<String>,
    pub(super) checked: bool,
}

/// One address Analyte can listen on: one of this PC's, every network card,
/// or the one saved before.
pub(in crate::admin::setup) struct Host {
    pub(in crate::admin::setup) value: String,
    /// The address, such as "192.168.1.20".
    pub(in crate::admin::setup) address: String,
    /// The adapter, or what the address means.
    pub(in crate::admin::setup) meta: String,
    pub(super) checked: bool,
}

/// The lab app picker of step 2.
pub(super) enum Picker {
    /// No lab app is set up yet.
    None,
    /// The lab apps could not be read; the text says why.
    Unreadable(String),
    /// The known lab apps; `chosen` is false when none is chosen yet.
    Choose { apps: Vec<LabApp>, chosen: bool },
}

/// One lab app to send results to.
pub(super) struct LabApp {
    pub(super) name: String,
    pub(super) address: String,
    pub(super) selected: bool,
}

/// GET /setup/{id}/test: step 3 before a test runs.
#[derive(askama::Template)]
#[template(path = "pages/setup/wizard/test.html")]
pub(super) struct TestStep {
    pub(super) frame: Frame,
    pub(super) wizard: Wizard,
    pub(super) foot: Foot,
    /// How to start the test; none for a watched folder, which has nothing to listen on.
    pub(super) start: Option<Start>,
}

/// The way on from step 3: Back to step 2 and Next to step 4, both carrying the choices.
pub(super) struct Foot {
    pub(super) back: String,
    pub(super) next: String,
    pub(super) hidden: Hidden,
    /// Next is the main action once a test ran or nothing can be tested.
    pub(super) primary: bool,
}

/// The form that starts (or repeats) the listen-only test.
pub(super) struct Start {
    pub(super) action: String,
    pub(super) csrf: String,
    pub(super) hidden: Hidden,
    /// What it listens on, in words with the exact setting on hover.
    pub(super) target: Hover,
    /// Analyte connects to the machine instead of listening.
    pub(super) connects: bool,
    /// How long it listens, in seconds.
    pub(super) limit: u64,
    pub(super) label: &'static str,
}

/// GET /setup/listen: step 3 while and after a test runs.
#[derive(askama::Template)]
#[template(path = "pages/setup/wizard/listen.html")]
pub(super) struct Listen {
    pub(super) frame: Frame,
    pub(super) wizard: Wizard,
    pub(super) foot: Foot,
    pub(super) live: Live,
}

/// What the test observed so far.
pub(super) struct Live {
    /// "Listening on 0.0.0.0:5000", "Serial port COM3".
    pub(super) target: String,
    /// The address of the device that connected, when Analyte listens (ADR 0026).
    pub(super) peer: Option<String>,
    /// Analyte connects to the machine instead of listening.
    pub(super) connects: bool,
    /// Time left, such as "0:41"; none once it ended.
    pub(super) clock: Option<String>,
    /// Seconds run and the time limit, for the progress bar.
    pub(super) elapsed: u64,
    pub(super) limit: u64,
    /// "1,204 bytes received", and when reading stopped at the size limit.
    pub(super) received: String,
    /// One line: the framing characters seen, such as "<ENQ> <STX> <CR>
    /// <ETX>", or that none were.
    pub(super) framing: Vec<String>,
    /// The test's state with what Analyte observed, such as "Listening on
    /// port 5000, nothing connected since 10:41".
    pub(super) state: Status,
    pub(super) verdict: Notice,
    /// Stop while it runs, with the session's form token.
    pub(super) stop: Option<String>,
    /// Test again once it ended.
    pub(super) again: Option<Start>,
}

/// GET /setup/{id}/save and POST /setup/{id}: step 4, what will be or was saved.
#[derive(askama::Template)]
#[template(path = "pages/setup/wizard/save.html")]
pub(super) struct SaveStep {
    pub(super) frame: Frame,
    pub(super) wizard: Wizard,
    pub(super) summary: Summary,
    pub(super) outcome: Outcome,
}

/// What will be saved, or was.
pub(super) struct Summary {
    pub(super) model: String,
    pub(super) connection: Hover,
    /// The serial cable settings in words, with what differs from the
    /// manual's suggestion; none unless it is on a serial cable.
    pub(super) cable: Option<String>,
    /// The model's own choices in words, with what differs from the manual's
    /// suggestion; none when it declares none (ADR 0032).
    pub(super) choices: Option<String>,
    /// Which devices it accepts; none unless the machine connects to this PC.
    pub(super) accepts: Option<Accepts>,
    /// The lab app it sends to; none while it records only (ADR 0034).
    pub(super) lab_app: Option<Sends>,
    pub(super) test: Tested,
    /// After saving: the new settings version and the one running now.
    pub(super) versions: Option<(Hover, Option<Hover>)>,
}

/// The lab app it sends results to, linked to its page, its address on hover.
pub(super) struct Sends {
    pub(super) name: String,
    pub(super) href: String,
    pub(super) address: String,
    /// Which of its keys the key source holds no value for, so results wait.
    pub(super) missing: Option<String>,
}

/// Which devices a machine that connects to this PC accepts, on step 4 (ADR 0026).
pub(super) enum Accepts {
    /// Before saving: a tick box, ticked, that keeps `value` as the allowed
    /// devices; `note` says what to check.
    Offer {
        value: String,
        label: String,
        note: Option<String>,
    },
    /// Before saving, with no device seen or typed: any device is accepted.
    Any,
    /// After saving: what was saved, and whether that is any device.
    Saved(String, bool),
}

/// The last connection test with exactly these choices.
pub(super) enum Tested {
    /// Its verdict and when it started.
    Seen {
        tone: Tone,
        title: String,
        when: Moment,
    },
    /// None ran with these choices since Analyte started.
    NotRun,
    /// The last test could not be read after the settings were saved.
    Unavailable,
    /// A watched folder has nothing to listen on.
    Folder,
}

/// Before or after saving.
pub(super) enum Outcome {
    /// The review: Back to step 3 and Save, each its own form.
    Review {
        back: String,
        /// Every choice, for Back.
        back_hidden: Hidden,
        action: String,
        csrf: String,
        hidden: Hidden,
    },
    /// Saved: what else happened, the problems, Restart now and the copies for support.
    Saved {
        csrf: String,
        lines: Vec<String>,
        problems: Vec<String>,
        support: Vec<String>,
        done: String,
    },
}
