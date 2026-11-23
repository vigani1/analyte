//! Carries and checks the setup choices posted by each step.

use super::{
    super::{choices, plan::Choice as Posted},
    page::Hidden,
};
use crate::admin::view::format;
use serde::Deserialize;

/// The choices made so far, as carried in a step's form. Every value is
/// re-checked before it is used; none is a secret. It reads only the fields
/// below and `choice.<name>`; any other field, or one given twice, is refused.
#[derive(Clone, Default, Deserialize)]
#[serde(try_from = "Vec<(String, String)>")]
pub(in crate::admin) struct Draft {
    /// A shipped model's `vendor/model` key, or `installed` to keep the current one.
    pub(in crate::admin::setup) definition: Option<String>,
    /// A detected serial port: `usb:<serial number>` or `name:<port name>`.
    pub(in crate::admin::setup) port: Option<String>,
    /// A typed serial port name; it wins over `port`.
    pub(in crate::admin::setup) port_name: Option<String>,
    pub(in crate::admin::setup) host: Option<String>,
    pub(in crate::admin::setup) tcp_port: Option<String>,
    pub(in crate::admin::setup) folder: Option<String>,
    pub(in crate::admin::setup) lab_app: Option<String>,
    /// The only devices a machine that connects to this PC may connect
    /// from, typed in step 2 or ticked in step 4 (ADR 0026); empty accepts any.
    pub(in crate::admin::setup) allowed_peers: Option<String>,
    /// The serial cable settings chosen in step 2 (ADR 0031), as the form
    /// values of [`super::super::cable::options`]; empty takes the definition's suggestion.
    pub(in crate::admin::setup) baud: Option<String>,
    pub(in crate::admin::setup) data_bits: Option<String>,
    pub(in crate::admin::setup) parity: Option<String>,
    pub(in crate::admin::setup) stop_bits: Option<String>,
    pub(in crate::admin::setup) flow_control: Option<String>,
    /// The values chosen in step 2 for the model's own choices (ADR 0032), by
    /// choice name, from the `choice.<name>` fields; empty takes the suggestion.
    pub(in crate::admin::setup) choices: std::collections::BTreeMap<String, String>,
    /// How Analyte uses the machine, chosen in step 1: `record` for record
    /// only (ADR 0034); empty connects it normally.
    pub(in crate::admin::setup) mode: Option<String>,
    /// Record only, step 2: the capture profile's questions (ADR 0034).
    /// Autolyser: seconds between next-report requests, and `yes` or `no`
    /// for reports that carry controls; VIDAS: `literal` or `alternate`.
    pub(in crate::admin::setup) poll_seconds: Option<String>,
    pub(in crate::admin::setup) vidas_patient_namespace: Option<String>,
    pub(in crate::admin::setup) controls: Option<String>,
    pub(in crate::admin::setup) acknowledgment: Option<String>,
    /// Record only, step 2: the mode the analyzer's own settings show, when
    /// the model's record file asks it (the cobas e 411's protocol type).
    pub(in crate::admin::setup) analyzer_mode: Option<String>,
}

/// Most fields a step's form carries: the fixed ones, the CSRF value and one
/// per choice a model may declare.
const MOST_FIELDS: usize = 20 + analyte_config::MAX_CHOICES;

impl TryFrom<Vec<(String, String)>> for Draft {
    type Error = String;

    fn try_from(fields: Vec<(String, String)>) -> Result<Self, Self::Error> {
        if fields.len() > MOST_FIELDS {
            return Err("too many fields".into());
        }
        let mut draft = Self::default();
        for (name, value) in fields {
            let earlier = match name.strip_prefix(choices::FIELD) {
                Some(choice) => draft.choices.insert(choice.to_owned(), value),
                None => draft
                    .slot(&name)
                    .ok_or_else(|| format!("unknown field `{name}`"))?
                    .replace(value),
            };
            if earlier.is_some() {
                return Err(format!("duplicate field `{name}`"));
            }
        }
        Ok(draft)
    }
}

/// The fields step 2 shows itself, besides the model's choices; every other
/// step carries them hidden.
pub(super) const CONNECTION_FIELDS: [&str; 17] = [
    "port",
    "port_name",
    "host",
    "tcp_port",
    "folder",
    "lab_app",
    PEERS,
    "baud",
    "data_bits",
    "parity",
    "stop_bits",
    "flow_control",
    "poll_seconds",
    "vidas_patient_namespace",
    "controls",
    "acknowledgment",
    "analyzer_mode",
];

/// The field naming the devices a listening machine accepts (ADR 0026).
pub(super) const PEERS: &str = "allowed_peers";

/// Stands for every `choice.<name>` field among the fields a step shows.
pub(super) const CHOICES: &str = "choice.*";

impl Draft {
    /// The field `name` of the form, by the names [`Draft::fields`] writes.
    fn slot(&mut self, name: &str) -> Option<&mut Option<String>> {
        Some(match name {
            "definition" => &mut self.definition,
            "port" => &mut self.port,
            "port_name" => &mut self.port_name,
            "host" => &mut self.host,
            "tcp_port" => &mut self.tcp_port,
            "folder" => &mut self.folder,
            "lab_app" => &mut self.lab_app,
            PEERS => &mut self.allowed_peers,
            "baud" => &mut self.baud,
            "data_bits" => &mut self.data_bits,
            "parity" => &mut self.parity,
            "stop_bits" => &mut self.stop_bits,
            "flow_control" => &mut self.flow_control,
            "mode" => &mut self.mode,
            "vidas_patient_namespace" => &mut self.vidas_patient_namespace,
            "poll_seconds" => &mut self.poll_seconds,
            "controls" => &mut self.controls,
            "acknowledgment" => &mut self.acknowledgment,
            "analyzer_mode" => &mut self.analyzer_mode,
            _ => return None,
        })
    }

    fn fields(&self) -> [(&'static str, Option<&str>); 19] {
        [
            ("definition", self.definition.as_deref()),
            ("port", self.port.as_deref()),
            ("port_name", self.port_name.as_deref()),
            ("host", self.host.as_deref()),
            ("tcp_port", self.tcp_port.as_deref()),
            ("folder", self.folder.as_deref()),
            ("lab_app", self.lab_app.as_deref()),
            (PEERS, self.allowed_peers.as_deref()),
            ("baud", self.baud.as_deref()),
            ("data_bits", self.data_bits.as_deref()),
            ("parity", self.parity.as_deref()),
            ("stop_bits", self.stop_bits.as_deref()),
            ("flow_control", self.flow_control.as_deref()),
            ("mode", self.mode.as_deref()),
            ("poll_seconds", self.poll_seconds.as_deref()),
            (
                "vidas_patient_namespace",
                self.vidas_patient_namespace.as_deref(),
            ),
            ("controls", self.controls.as_deref()),
            ("acknowledgment", self.acknowledgment.as_deref()),
            ("analyzer_mode", self.analyzer_mode.as_deref()),
        ]
    }

    /// Every choice made, except the ones the step shows, as hidden fields;
    /// [`CHOICES`] among `shown` leaves out the model's own choices.
    pub(super) fn hidden(&self, shown: &[&str]) -> Hidden {
        let fixed = self
            .fields()
            .into_iter()
            .filter(|(name, _)| !shown.contains(name))
            .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())));
        let own = self
            .choices
            .iter()
            .filter(|_| !shown.contains(&CHOICES))
            .map(|(name, value)| (format!("{}{name}", choices::FIELD), value.clone()));
        fixed.chain(own).collect()
    }

    /// Whether `other` holds the same choices for the connection test; an
    /// empty field counts as not chosen, as every step reads it. The allowed
    /// devices do not change what the test listens on, so step 4 still shows
    /// the test after they are ticked or typed.
    pub(super) fn same(&self, other: &Self) -> bool {
        let chosen = |draft: &Self| -> Vec<(String, String)> {
            draft
                .hidden(&[PEERS])
                .into_iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .collect()
        };
        chosen(self) == chosen(other)
    }

    /// Every choice made, as a URL query that opens a step with them.
    pub(super) fn query(&self) -> String {
        self.hidden(&[])
            .iter()
            .map(|(name, value)| format!("{name}={}", format::path_segment(value)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

impl From<Posted> for Draft {
    fn from(choice: Posted) -> Self {
        choice.draft
    }
}
