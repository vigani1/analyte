//! Holds the shared LAW error type and validates QBP work queries (`PaLM` TF-2b 3.27.4); evidence level: documented.
//!
//! These views borrow the original message. A validation error never consumes it.
//! Query responses, work downloads and results have separate pure APIs. Full
//! field/option conformance is still incomplete; see the crate README.
/// Maximum manager-authored AWOS identity bytes (TF-2b C.6/C.11, OBR-2).
pub const AWOS_ID_MAX_BYTES: usize = 50;

use crate::{Field, Message, Msh, Segment, Version};
use thiserror::Error;

/// A LAW validation failure, without payload in its diagnostic.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// Validation retained the actual one-based occurrence of the offending segment.
    #[error("LAW segment occurrence {occurrence}: {source}")]
    AtSegment {
        /// Occurrence among segments with the same identifier, across the whole message.
        occurrence: usize,
        /// Original specific validation failure; wrapped at most once by validation.
        #[source]
        source: Box<Error>,
    },
    /// The app has not supplied a matching test/type/factor definition.
    #[error("unresolved dilution definition at {segment}-{field}.{component}")]
    DilutionDefinition {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field position.
        field: usize,
        /// One-based component position.
        component: usize,
    },

    /// A nested primitive violates its profiled usage, length or datatype.
    #[error("LAW subcomponent rejected at {segment}-{field}.{component}.{subcomponent}")]
    Subcomponent {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field position.
        field: usize,
        /// One-based component position.
        component: usize,
        /// One-based subcomponent position.
        subcomponent: usize,
    },
    /// Local and universal authority names need an app-supplied binding; never guess.
    #[error("unresolved LAW authority at {segment}-{field}.{component}")]
    Authority {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field position.
        field: usize,
        /// One-based authority component position.
        component: usize,
    },

    /// A composite component violates its profiled usage, length or datatype.
    #[error("LAW component rejected at {segment}-{field}.{component}")]
    Component {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field position.
        field: usize,
        /// One-based component position.
        component: usize,
    },

    /// Individually valid fields contradict each other; reject the whole content.
    #[error("inconsistent LAW content at {segment}-{field}")]
    Inconsistent {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field.
        field: usize,
    },
    /// Base HL7 control validation failed.
    #[error(transparent)]
    Syntax(#[from] crate::Error),

    /// A required field, cardinality or predicate was violated.
    #[error("LAW content rejected at {segment}-{field}")]
    Content {
        /// Segment identifier.
        segment: &'static str,
        /// One-based field, or zero for segment cardinality/order.
        field: usize,
    },
}

impl Error {
    pub(super) fn base(&self) -> &Self {
        let mut error = self;
        while let Self::AtSegment { source, .. } = error {
            error = source;
        }
        error
    }
    pub(super) fn occurrence(&self) -> Option<usize> {
        if let Self::AtSegment { occurrence, .. } = self {
            Some(*occurrence)
        } else {
            None
        }
    }
    // Only attach the current segment when the failure identifies that segment.
    // Missing groups and errors referring to an earlier group remain unlocated.
    pub(super) fn at_segment(self, segment: Segment<'_>, occurrence: usize) -> Self {
        let named = match self.base() {
            Self::DilutionDefinition { segment, .. }
            | Self::Subcomponent { segment, .. }
            | Self::Authority { segment, .. }
            | Self::Component { segment, .. }
            | Self::Inconsistent { segment, .. }
            | Self::Content { segment, .. }
            | Self::Syntax(crate::Error::Control { segment, .. }) => Some(*segment),
            Self::AtSegment { .. } | Self::Syntax(_) => None,
        };
        if self.occurrence().is_none() && named.is_some_and(|name| name.as_bytes() == segment.id())
        {
            Self::AtSegment {
                occurrence,
                source: Box::new(self),
            }
        } else {
            self
        }
    }
}

fn content(segment: &'static str, field: usize) -> Error {
    Error::Content { segment, field }
}

fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}

fn required(segment: Segment<'_>, field: usize, maximum: usize) -> Result<&[u8], Error> {
    let value = raw(segment, field);
    if value.is_empty() || value == b"\"\"" || value.len() > maximum {
        return Err(content("QPD", field));
    }
    Ok(value)
}

/// A container/carrier identity with original namespace components preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity<'a>(&'a [u8]);

impl<'a> Identity<'a> {
    /// Returns the original EI bytes, including any assigning namespace.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.0
    }

    /// Returns the identifier component, preserving its original bytes.
    #[must_use]
    pub fn value(self) -> &'a [u8] {
        self.0
            .split(|byte| *byte == b'^')
            .next()
            .unwrap_or_default()
    }

    /// Checks the local assigning namespace and absence of other nonempty components.
    #[must_use]
    pub fn is_local_to(self, namespace: &[u8]) -> bool {
        let mut parts = self.0.split(|byte| *byte == b'^');
        let _ = parts.next();
        let authority = parts.next().unwrap_or_default();
        (authority.is_empty() || authority == namespace) && parts.all(<[u8]>::is_empty)
    }
}

/// Query scope, with informational position distinct from lookup position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// Work for an identified container.
    Container(Identity<'a>),
    /// Every pending AWOS.
    AllPending,
    /// Lookup by carrier and position, with optional location.
    Rack {
        /// Carrier identity.
        carrier: Identity<'a>,
        /// Original numeric array, preserving all axes.
        position: &'a [u8],
        /// Additional coded location, when supplied.
        location: Option<&'a [u8]>,
    },
    /// Lookup by tray and position, with optional location.
    Tray {
        /// Tray identity.
        tray: Identity<'a>,
        /// Original numeric array, preserving all axes.
        position: &'a [u8],
        /// Additional coded location, when supplied.
        location: Option<&'a [u8]>,
    },
    /// An isolate and its primary container are both lookup evidence.
    Isolate {
        /// Isolate container identity.
        isolate: Identity<'a>,
        /// Primary container identity.
        parent: Identity<'a>,
    },
    /// Lookup uses the container; position is informational only.
    InformationalRack {
        /// Container used for lookup.
        container: Identity<'a>,
        /// Informational carrier identity.
        carrier: Identity<'a>,
        /// Informational position.
        position: &'a [u8],
    },
}

/// Validated QBP structure and query field predicates; original bytes remain borrowed.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    message: &'a Message,
    qpd: Segment<'a>,
    scope: Scope<'a>,
}

impl<'a> Query<'a> {
    /// Validates QBP structure, LAW header, QPD scope and immediate response mode.
    ///
    /// # Errors
    /// Returns a located content error or the underlying HL7 control error.
    /// This is not a complete LAW profile validator (see the crate README).
    pub fn new(message: &'a Message) -> Result<Self, Error> {
        validate_query_header(message)?;
        let d = message.delimiters();
        let mut segments = message.segments().skip(1);
        let qpd = segments
            .next()
            .filter(|s| s.id() == b"QPD")
            .ok_or(content("QPD", 0))?;
        let rcp = segments
            .next()
            .filter(|s| s.id() == b"RCP")
            .ok_or(content("RCP", 0))?;
        if segments.next().is_some() {
            return Err(content("RCP", 0));
        }
        validate_query_response_control(rcp)?;
        let name = required(qpd, 1, 60)?;
        let mut name_parts = name.split(|b| *b == d.component);
        let kind = name_parts.next().unwrap_or_default();
        let description = name_parts.next().unwrap_or_default();
        if description.is_empty()
            || name_parts.next() != Some(b"IHELAW".as_slice())
            || name_parts.next().is_some()
        {
            return Err(content("QPD", 1));
        }
        validate_query_description(kind, description)?;
        required(qpd, 2, 32)?;
        let mandatory: &[usize] = match kind {
            b"WOS" => &[3],
            b"WOS_ALL" => &[],
            b"WOS_BY_RACK" => &[4, 5],
            b"WOS_BY_TRAY" => &[6, 7],
            b"WOS_BY_ISOLATE" => &[3, 9],
            b"WOS_INFORMATIONAL_RACK" => &[3, 4, 5],
            _ => return Err(content("QPD", 1)),
        };
        for field in 3..=9 {
            if mandatory.contains(&field) {
                required(qpd, field, 80)?;
            } else if !(field == 8 && matches!(kind, b"WOS_BY_RACK" | b"WOS_BY_TRAY"))
                && !raw(qpd, field).is_empty()
            {
                return Err(content("QPD", field));
            }
        }
        for (offset, field) in qpd.fields().enumerate() {
            if offset >= 9 && !field.raw().is_empty() {
                return Err(content("QPD", 0));
            }
            if field.repetitions().count() != 1 || field.is_null() {
                return Err(content("QPD", offset.saturating_add(1)));
            }
        }
        for field in mandatory {
            if matches!(field, 5 | 7) {
                for axis in raw(qpd, *field).split(|b| *b == d.component) {
                    if axis.is_empty() || axis.len() > 16 || !axis.iter().all(u8::is_ascii_digit) {
                        return Err(content("QPD", *field));
                    }
                }
            } else if raw(qpd, *field)
                .split(|b| *b == d.component)
                .next()
                .is_none_or(|v| v.is_empty() || v.len() > 50)
            {
                return Err(content("QPD", *field));
            }
        }
        let location = (!raw(qpd, 8).is_empty()).then_some(raw(qpd, 8));
        validate_query_location(location, d.component)?;
        let scope = match kind {
            b"WOS" => Scope::Container(Identity(raw(qpd, 3))),
            b"WOS_ALL" => Scope::AllPending,
            b"WOS_BY_RACK" => Scope::Rack {
                carrier: Identity(raw(qpd, 4)),
                position: raw(qpd, 5),
                location,
            },
            b"WOS_BY_TRAY" => Scope::Tray {
                tray: Identity(raw(qpd, 6)),
                position: raw(qpd, 7),
                location,
            },
            b"WOS_BY_ISOLATE" => Scope::Isolate {
                isolate: Identity(raw(qpd, 3)),
                parent: Identity(raw(qpd, 9)),
            },
            b"WOS_INFORMATIONAL_RACK" => Scope::InformationalRack {
                container: Identity(raw(qpd, 3)),
                carrier: Identity(raw(qpd, 4)),
                position: raw(qpd, 5),
            },
            _ => return Err(content("QPD", 1)),
        };
        Ok(Self {
            message,
            qpd,
            scope,
        })
    }

    /// Returns the validated lookup scope.
    #[must_use]
    pub fn scope(self) -> Scope<'a> {
        self.scope
    }

    /// Returns the exact query tag, distinct from MSH-10.
    #[must_use]
    pub fn tag(self) -> &'a [u8] {
        raw(self.qpd, 2)
    }

    /// Returns the original message for protected retention and exact QPD echoing.
    #[must_use]
    pub fn message(self) -> &'a Message {
        self.message
    }
}

mod responses;

pub use responses::{CacheAnswer, QueryFollowup, QueryResponse, query_response};

mod work;

pub use work::{
    Address, Awos, AwosDecision, AwosOutcome, Capabilities, Instructions, PeerContent,
    PeerInstructions, PeerRepeat, WorkAction, WorkCode, WorkFields, WorkResponse, negative_work,
    work_order, work_order_with_profile,
};

mod results;

pub use results::{
    Attached, Observation, Parent, ResultStatus, Results, SpecimenObservation, Value,
    result_acknowledgment,
};

mod envelope;
mod fields;

pub use envelope::{ControlDirection, message_type, parse_message, validate_control};

mod patient;

mod specimen;

mod order;

mod tcd;

pub use order::{OrderControl, OrderStatus};

pub use tcd::{Dilution, DilutionBinding, DilutionMeaning, ResultAdjustment, TestDetail};

mod profile;

pub use profile::{FieldRule, Profile, Usage, validate_segment};

pub use patient::AuthorityBinding;

fn msh(message: &Message) -> Result<Msh<'_>, Error> {
    let segment = message.segments().next().ok_or(content("MSH", 0))?;
    envelope::validate_segment(segment, ControlDirection::Receive)?;
    let kind = raw(segment, 9)
        .split(|b| *b == b'^')
        .next()
        .unwrap_or_default();
    Ok(Msh::from_law(
        segment,
        matches!(kind, b"QBP" | b"OML" | b"OUL"),
    ))
}

/// Validates a LAW ST message identity (TF-2b C.4, at most 50 characters).
/// # Errors
/// Rejects empty, null, oversized or control-containing IDs and unescaped delimiters.
pub fn control_id(raw: &[u8]) -> Result<crate::ControlId<'_>, crate::Error> {
    fields::identity(raw).map_err(|_| crate::Error::Control {
        segment: "MSH",
        field: 10,
    })?;
    Ok(crate::ControlId::from_profile(raw))
}

mod failure;

pub use failure::{
    ErrorLocation, Failure, receive_acknowledgment, receive_query, receive_results,
    receive_work_response,
};

mod content;

pub use content::{ContainerDetails, OrderDetails, PatientInfo, SpecimenDetails, Visit, WorkNotes};

mod observation;

mod rejection;
pub use rejection::reject_trigger;

mod status;
pub use status::{SINGLE_RUN_STATUSES, STATUS_TRANSITIONS, WITHHELD_STATUSES, observation_run};

impl From<crate::wire::Limit> for Error {
    fn from(_: crate::wire::Limit) -> Self {
        crate::Error::Limit {
            kind: "LAW output",
            segment: 0,
            field: 0,
        }
        .into()
    }
}

fn validate_query_header(message: &Message) -> Result<(), Error> {
    profile::header(message, b"LAB-27", true)?;
    let msh = msh(message)?;
    if msh.version() != Version::V251 {
        return Err(content("MSH", 12));
    }
    let header = msh.segment();
    let d = message.delimiters();
    let components: Vec<_> = raw(header, 9)
        .split(|b| *b == d.component)
        .take(3)
        .collect();
    if components != [b"QBP".as_slice(), b"Q11", b"QBP_Q11"] {
        return Err(content("MSH", 9));
    }
    for (field, value) in [(15, b"NE".as_slice()), (16, b"AL")] {
        if raw(header, field) != value {
            return Err(content("MSH", field));
        }
    }
    let profile = raw(header, 21)
        .split(|b| *b == d.repetition)
        .next()
        .unwrap_or_default();
    if !profile
        .split(|b| *b == d.component)
        .take(2)
        .eq([b"LAB-27".as_slice(), b"IHE"])
    {
        return Err(content("MSH", 21));
    }
    Ok(())
}

fn validate_query_response_control(rcp: Segment<'_>) -> Result<(), Error> {
    if raw(rcp, 1) != b"I" {
        return Err(content("RCP", 1));
    }
    let modality: Vec<_> = raw(rcp, 3)
        .split(|b| *b == rcp.delimiters.component)
        .collect();
    if modality.first().copied() != Some(b"R".as_slice())
        || modality.get(2).copied() != Some(b"HL70394".as_slice())
        || modality.len() != 3
        || !matches!(modality.get(1).copied(), Some(b"" | b"Real Time"))
    {
        return Err(content("RCP", 3));
    }
    for (offset, field) in rcp.fields().enumerate().skip(1) {
        // RCP-2 and fields after RCP-3 are excluded by the LAW static definition.
        if offset != 2 && !field.raw().is_empty() {
            return Err(content("RCP", 0));
        }
    }
    Ok(())
}

fn validate_query_description(kind: &[u8], description: &[u8]) -> Result<(), Error> {
    let expected_description: &[u8] = match kind {
        b"WOS" => b"Work Order Step",
        b"WOS_ALL" => b"Work Order Step All",
        b"WOS_BY_RACK" => b"Work Order Step by Rack",
        b"WOS_BY_TRAY" => b"Work Order Step by Tray",
        b"WOS_BY_ISOLATE" => b"Work Order Step by Isolate",
        b"WOS_INFORMATIONAL_RACK" => b"Query by WOS and Informational Rack",
        _ => return Err(content("QPD", 1)),
    };
    if description != expected_description {
        return Err(content("QPD", 1));
    }
    Ok(())
}

fn validate_query_location(location: Option<&[u8]>, component: u8) -> Result<(), Error> {
    if let Some(location) = location {
        let parts: Vec<_> = location.split(|b| *b == component).collect();
        let part = |n: usize| parts.get(n).copied().unwrap_or_default();
        fields::primitive(part(0), true, 20, content("QPD", 8))?;
        fields::primitive(part(1), false, 199, content("QPD", 8))?;
        if part(0).is_empty()
            || part(2).len() != 5
            || part(2).len() > 12
            || !part(2).starts_with(b"99")
            || !part(2).iter().all(u8::is_ascii_alphanumeric)
        {
            return Err(content("QPD", 8));
        }
    }
    Ok(())
}
