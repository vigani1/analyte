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
pub(super) type Hidden = Vec<(&'static str, String)>;

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
    pub(super) lab_apps: Picker,
}

/// The connection fields the model's transport asks for.
pub(super) enum Inputs {
    /// The detected ports, or why they could not be listed, then a typed name.
    Serial {
        ports: Result<Vec<Port>, String>,
        typed: String,
        placeholder: String,
    },
    /// Analyte listens and the machine connects: which of this PC's
    /// addresses to listen on (or why they could not be listed, then a typed
    /// address), and the port.
    Listens {
        hosts: Result<Vec<Host>, String>,
        typed: String,
        port: String,
    },
    /// Analyte connects to the machine: its IP address and port.
    Connects { host: String, port: String },
    /// A folder Analyte watches.
    Folder(String),
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
    pub(super) lab_app: Sends,
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
    /// A watched folder has nothing to listen on.
    Folder,
}

/// Before or after saving.
pub(super) enum Outcome {
    /// The review: Back to step 3 and Save, each its own form.
    Review {
        back: String,
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
