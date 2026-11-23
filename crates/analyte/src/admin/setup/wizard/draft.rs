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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Machine,
    Connection,
}

/// Declares each fixed field once, in the order used for hidden fields.
macro_rules! fields {
    ($($field:ident: $page:ident),+ $(,)?) => {
        // One slot for CSRF, and one per choice a model may declare.
        const MOST_FIELDS: usize = [$(stringify!($field)),+].len() + 1 + analyte_config::MAX_CHOICES;

        impl Draft {
            fn slot(&mut self, name: &str) -> Option<&mut Option<String>> {
                match name {
                    $(stringify!($field) => Some(&mut self.$field),)+
                    _ => None,
                }
            }

            fn fields(&self) -> impl Iterator<Item = (&'static str, Option<&str>, Page)> {
                [$((stringify!($field), self.$field.as_deref(), Page::$page)),+].into_iter()
            }
        }
    };
}

fields! {
    definition: Machine,
    port: Connection,
    port_name: Connection,
    host: Connection,
    tcp_port: Connection,
    folder: Connection,
    lab_app: Connection,
    allowed_peers: Connection,
    baud: Connection,
    data_bits: Connection,
    parity: Connection,
    stop_bits: Connection,
    flow_control: Connection,
    mode: Machine,
    poll_seconds: Connection,
    vidas_patient_namespace: Connection,
    controls: Connection,
    acknowledgment: Connection,
    analyzer_mode: Connection,
}

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

/// The field naming the devices a listening machine accepts (ADR 0026).
pub(super) const PEERS: &str = stringify!(allowed_peers);

impl Draft {
    fn hidden_fixed(&self, shown: impl Fn(&str, Page) -> bool) -> Hidden {
        self.fields()
            .filter(|(name, _, page)| !shown(name, *page))
            .filter_map(|(name, value, _)| value.map(|value| (name.to_owned(), value.to_owned())))
            .collect()
    }

    /// Every chosen field that the step does not show.
    pub(super) fn hidden(&self, shown: &[&str]) -> Hidden {
        let mut fixed = self.hidden_fixed(|name, _| shown.contains(&name));
        fixed.extend(
            self.choices
                .iter()
                .map(|(name, value)| (format!("{}{name}", choices::FIELD), value.clone())),
        );
        fixed
    }

    /// Step 2 renders the connection fields and model choices itself.
    pub(super) fn hidden_for_connection(&self) -> Hidden {
        self.hidden_fixed(|_, page| page == Page::Connection)
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
