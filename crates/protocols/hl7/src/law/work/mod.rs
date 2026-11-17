//! Parses LAW work messages without order lifecycle decisions.
use super::content::{ContainerDetails, OrderDetails, PatientInfo, SpecimenDetails, WorkNotes};
use super::{Error, Identity, content};
mod encode;
mod response;
pub use encode::{Address, negative_work, work_order, work_order_with_profile};
pub use response::{AwosDecision, AwosOutcome, WorkResponse};

/// Independent peer content and instruction options; absence never authorizes clinical decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    /// Negotiated patient, specimen and container content.
    pub content: PeerContent,
    /// Negotiated work instructions.
    pub instructions: PeerInstructions,
}
/// Independent negotiated content options.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeerContent {
    /// Peer accepts patient/encounter demographics.
    pub patient: bool,
    /// Peer accepts extended container characteristics.
    pub container: bool,
    /// Peer accepts supplied specimen identity and characteristics.
    pub specimen: bool,
}
/// Independent negotiated repeat options.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeerRepeat {
    /// Peer accepts app-authorized manager rerun/reflex status.
    pub manager_repeat: bool,
    /// Peer accepts app-authorized automatic repeat/reflex permissions.
    pub repeat_control: bool,
}
/// Independent negotiated instructions options.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeerInstructions {
    /// Peer accepts explicit requested dilution.
    pub dilution: bool,
    /// Peer accepts explicit priority.
    pub priority: bool,
    /// Independently negotiated repeat status and permissions.
    pub repeat: PeerRepeat,
}

/// New work versus cancellation of a previously sent AWOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkAction {
    /// New AWOS; replicates require separate unique IDs.
    New,
    /// Cancel request; does not itself establish cancellation.
    Cancel,
}

/// App-supplied optional instructions; no defaults are synthesized.
#[derive(Debug, Clone, Copy, Default)]
pub struct Instructions<'a> {
    /// App-authorized dilution to use if the analyzer later initiates a rerun.
    pub rerun_dilution: Option<&'a [u8]>,
    /// Known endogenous content of a pre-dilution diluent.
    pub predilution_content: Option<&'a [u8]>,
    /// Requested specimen consumption, with its supplied units.
    pub consumption: Option<&'a [u8]>,
    /// Explicit coded auto-dilution type supplied by the app.
    pub dilution_type: Option<&'a [u8]>,
    /// Requested auto-dilution factor as an HL7 SN composite.
    pub dilution: Option<&'a [u8]>,
    /// App-authorized repeat status as a coded HL7 0389 value.
    pub repeat: Option<&'a [u8]>,
    /// Explicit app permission for automatic repeat (Y/N).
    pub automatic_repeat: Option<bool>,
    /// Explicit app permission for reflex testing (Y/N).
    pub reflex: Option<bool>,
    /// App-supplied coded priority.
    pub priority: Option<&'a [u8]>,
}

/// Unencoded code, display text and coding system for a LAW coded field.
#[derive(Debug, Clone, Copy)]
pub struct WorkCode<'a> {
    /// Code as supplied by the mapping.
    pub code: &'a str,
    /// Optional display text, including an explicitly empty value.
    pub text: &'a str,
    /// Explicit coding system.
    pub system: &'a str,
}

/// Encoded required LAW composites, owned independently of the source mapping.
#[derive(Debug)]
pub struct WorkFields {
    specimen: String,
    specimen_type: String,
    test: String,
    priority: Option<String>,
}

impl WorkFields {
    /// Encodes an EIP specimen identity and coded specimen/test/priority fields.
    /// # Errors
    /// Rejects separator injection or fields beyond LAW's existing wire bounds.
    pub fn new(
        specimen: &str,
        namespace: &str,
        specimen_type: WorkCode<'_>,
        test: WorkCode<'_>,
        priority: Option<&str>,
    ) -> Result<Self, Error> {
        fn join(parts: &[&str], separator: &str, max: usize) -> Result<String, Error> {
            let mut length = 0usize;
            for (index, part) in parts.iter().enumerate() {
                if !part.is_empty()
                    && !crate::is_safe_field(part.as_bytes(), max, crate::FieldSyntax::Plain)
                {
                    return Err(content("OBR", 4));
                }
                length = length
                    .checked_add(part.len())
                    .and_then(|n| n.checked_add(if index == 0 { 0 } else { separator.len() }))
                    .ok_or(content("OBR", 4))?;
                if length > max {
                    return Err(content("OBR", 4));
                }
            }
            Ok(parts.join(separator))
        }
        Ok(Self {
            specimen: join(&[specimen, namespace], "&", 427)?,
            specimen_type: join(
                &[specimen_type.code, specimen_type.text, specimen_type.system],
                "^",
                250,
            )?,
            test: join(&[test.code, test.text, test.system], "^", 250)?,
            priority: priority
                .map(|code| join(&[code, "", "HL70485"], "^", 250))
                .transpose()?,
        })
    }

    /// Borrows these composites for an AWOS, applying the existing required-field checks.
    /// # Errors
    /// Returns the same required identifier, specimen and test errors as [`Awos::new`].
    pub fn awos<'a>(
        &'a self,
        id: &'a [u8],
        role: &'a [u8],
        container: &'a [u8],
        action: WorkAction,
    ) -> Result<Awos<'a>, Error> {
        Ok(Awos::new(
            id,
            self.specimen.as_bytes(),
            self.specimen_type.as_bytes(),
            role,
            container,
            self.test.as_bytes(),
            action,
        )?
        .with_instructions(Instructions {
            priority: self.priority.as_deref().map(str::as_bytes),
            ..Instructions::default()
        }))
    }
}

/// One app-authorized battery/test under one unique AWOS ID.
#[derive(Debug, Clone, Copy)]
pub struct Awos<'a> {
    notes: Option<&'a WorkNotes<'a>>,
    specimen_details: Option<&'a SpecimenDetails<'a>>,
    container_details: Option<&'a ContainerDetails<'a>>,
    order_details: Option<&'a OrderDetails<'a>>,
    id: &'a [u8],
    specimen: &'a [u8],
    specimen_type: &'a [u8],
    role: &'a [u8],
    container: &'a [u8],
    test: &'a [u8],
    action: WorkAction,
    instructions: Instructions<'a>,
    transaction_time: Option<&'a [u8]>,
}

impl<'a> Awos<'a> {
    /// Validates the required identifiers and coded specimen/test fields.
    /// An AWOS orders one test or battery; separate replicates need separate IDs.
    /// # Errors
    /// Rejects null, oversized or separator-injected fields. An empty container
    /// requires parent and position details at rendering (C.11).
    pub fn new(
        id: &'a [u8],
        specimen: &'a [u8],
        specimen_type: &'a [u8],
        role: &'a [u8],
        container: &'a [u8],
        test: &'a [u8],
        action: WorkAction,
    ) -> Result<Self, Error> {
        for (value, segment, field, maximum) in [
            (id, "OBR", 2, super::AWOS_ID_MAX_BYTES),
            (specimen, "SPM", 2, 427),
            (container, "SAC", 3, 80),
            (test, "OBR", 4, 250),
        ] {
            if field != 3 || segment != "SAC" || !value.is_empty() {
                safe(value, segment, field, maximum)?;
            }
        }
        match role {
            b"P" => safe(specimen_type, "SPM", 4, 250)?,
            b"Q" => {
                if specimen_type != b"\"\"" {
                    return Err(content("SPM", 4));
                }
            }
            _ => return Err(content("SPM", 11)),
        }
        Ok(Self {
            notes: None,
            specimen_details: None,
            container_details: None,
            order_details: None,
            id,
            specimen,
            specimen_type,
            role,
            container,
            test,
            action,
            instructions: Instructions::default(),
            transaction_time: None,
        })
    }

    /// Attaches explicit app instructions, validated against peer capabilities at rendering.
    #[must_use]
    pub fn with_instructions(mut self, instructions: Instructions<'a>) -> Self {
        self.instructions = instructions;
        self
    }

    /// Supplies the actual initiating-event timestamp for ORC-9, without a timezone.
    /// If omitted, the caller asserts that the supplied response time is also the
    /// initiating-event time. Earlier order events must supply their own timestamp.
    #[must_use]
    pub fn with_transaction_time(mut self, timestamp: &'a [u8]) -> Self {
        self.transaction_time = Some(timestamp);
        self
    }

    /// Supplies comments with explicit group ownership; text is encoded FT data.
    #[must_use]
    pub fn with_notes(mut self, notes: &'a WorkNotes<'a>) -> Self {
        self.notes = Some(notes);
        self
    }

    /// Supplies available enhanced specimen fields; rendering checks the peer profile.
    #[must_use]
    pub fn with_specimen_details(mut self, details: &'a SpecimenDetails<'a>) -> Self {
        self.specimen_details = Some(details);
        self
    }

    /// Supplies available container characteristics without converting volume or dilution.
    #[must_use]
    pub fn with_container_details(mut self, details: &'a ContainerDetails<'a>) -> Self {
        self.container_details = Some(details);
        self
    }

    /// Supplies order grouping/provider context without changing AWOS identity.
    #[must_use]
    pub fn with_order_details(mut self, details: &'a OrderDetails<'a>) -> Self {
        self.order_details = Some(details);
        self
    }

    /// Returns the stable AWOS identity; it is never generated by rendering.
    #[must_use]
    pub fn id(self) -> &'a [u8] {
        self.id
    }
}

fn safe(value: &[u8], segment: &'static str, field: usize, maximum: usize) -> Result<(), Error> {
    if !crate::is_safe_field(value, maximum, crate::FieldSyntax::Composite) {
        return Err(content(segment, field));
    }
    Ok(())
}
