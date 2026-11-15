use super::{Error, invalid};

pub(super) fn scalar(
    value: &[u8],
    segment: &str,
    field: usize,
    maximum: usize,
    required: bool,
) -> Result<(), Error> {
    if (required && value.is_empty())
        || value == b"\"\""
        || value.len() > maximum
        || value
            .iter()
            .any(|b| b.is_ascii_control() || b"|^~\\&".contains(b))
    {
        return Err(invalid(segment, field));
    }
    Ok(())
}

/// Namespaced EI order identity. It is never generated from a sample ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderId<'a> {
    pub(super) id: &'a [u8],
    pub(super) namespace: &'a [u8],
    pub(super) universal: Option<(&'a [u8], &'a [u8])>,
}

impl<'a> OrderId<'a> {
    /// Checks a local EI with its explicitly supplied assigning namespace.
    /// # Errors
    /// Rejects empty/injected components or an EI exceeding ORC-2/3's 22 bytes.
    pub fn new(id: &'a [u8], namespace: &'a [u8]) -> Result<Self, Error> {
        Self::with_universal(id, namespace, None)
    }

    /// Checks complete EI components, retaining the supplied universal namespace.
    /// # Errors
    /// Rejects invalid components or a universal ID without its type (and conversely).
    pub fn with_universal(
        id: &'a [u8],
        namespace: &'a [u8],
        universal: Option<(&'a [u8], &'a [u8])>,
    ) -> Result<Self, Error> {
        scalar(id, "EI", 1, 22, true)?;
        scalar(namespace, "EI", 2, 20, universal.is_none())?;
        let mut length = id
            .len()
            .checked_add(namespace.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Limit)?;
        if let Some((identifier, kind)) = universal {
            scalar(identifier, "EI", 3, 199, true)?;
            scalar(kind, "EI", 4, 6, true)?;
            if !crate::datatypes::uid(kind, crate::Version::V251) {
                return Err(invalid("EI", 4));
            }
            length = length
                .checked_add(identifier.len())
                .and_then(|n| n.checked_add(kind.len()))
                .and_then(|n| n.checked_add(2))
                .ok_or(Error::Limit)?;
        }
        if length > 22 {
            return Err(invalid("EI", 1));
        }
        Ok(Self {
            id,
            namespace,
            universal,
        })
    }

    /// Returns the identifier spelling without numeric conversion.
    #[must_use]
    pub fn identifier(self) -> &'a [u8] {
        self.id
    }

    /// Returns the local assigning namespace, if supplied.
    #[must_use]
    pub fn namespace(self) -> &'a [u8] {
        self.namespace
    }
}

/// Literal sample identity, never a number or a fallback order identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleId<'a>(&'a [u8]);

impl<'a> SampleId<'a> {
    /// Checks a nonempty literal identity; reserved delimiters are escaped on output.
    /// # Errors
    /// Rejects null, control bytes or values longer than the supported placer field.
    pub fn new(value: &'a [u8]) -> Result<Self, Error> {
        if value.is_empty()
            || value == b"\"\""
            || value.len() > 60
            || value.iter().any(u8::is_ascii_control)
        {
            return Err(invalid("OBR", 18));
        }
        Ok(Self(value))
    }

    /// Returns exactly the caller's sample bytes, including leading zeros and case.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.0
    }
}

/// Explicitly agreed sample placement; neither field has a standard barcode meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleField {
    /// OBR-18, the first placer-owned text field.
    Placer1,
    /// OBR-19, the second placer-owned text field.
    Placer2,
}

/// A CE test or specimen code with its supplied coding system.
#[derive(Debug, Clone, Copy)]
pub struct Code<'a> {
    pub(super) identifier: &'a [u8],
    pub(super) text: &'a [u8],
    pub(super) system: &'a [u8],
    pub(super) version: Option<&'a [u8]>,
}

impl<'a> Code<'a> {
    /// Validates CE components without guessing a coding system from a label.
    /// # Errors
    /// Rejects missing code/system, delimiter injection or component length violations.
    pub fn new(identifier: &'a [u8], text: &'a [u8], system: &'a [u8]) -> Result<Self, Error> {
        scalar(identifier, "CE", 1, 20, true)?;
        scalar(text, "CE", 2, 199, false)?;
        scalar(system, "CE", 3, 20, true)?;
        Ok(Self {
            identifier,
            text,
            system,
            version: None,
        })
    }

    /// Supplies a CWE coding-system version without guessing it from the code.
    /// CE destinations cannot represent this metadata and reject it explicitly.
    /// # Errors
    /// Rejects missing, oversized or injected version strings.
    pub fn with_version(mut self, version: &'a [u8]) -> Result<Self, Error> {
        scalar(version, "CWE", 7, 10, true)?;
        self.version = Some(version);
        Ok(self)
    }
}

/// A supplied TS retaining precision and timezone without filling missing parts.
#[derive(Debug, Clone, Copy)]
pub struct Timestamp<'a>(&'a [u8]);

impl<'a> Timestamp<'a> {
    /// Checks the lexical/calendar rules used by legacy TS values.
    /// # Errors
    /// Rejects invalid or delimiter-injected timestamps.
    pub fn new(value: &'a [u8]) -> Result<Self, Error> {
        scalar(value, "TS", 1, 24, true)?;
        if !crate::datatypes::ts(&[value]) {
            return Err(invalid("TS", 1));
        }
        Ok(Self(value))
    }

    /// Returns the original timestamp spelling.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.0
    }
}

/// Supported explicit TQ priorities; no routine priority is synthesized.
#[derive(Debug, Clone, Copy)]
pub enum Priority {
    /// Routine.
    Routine,
    /// Stat.
    Stat,
    /// As soon as possible.
    Asap,
    /// Before operation.
    Preop,
    /// Callback.
    Callback,
    /// Timing critical.
    TimingCritical,
    /// As needed.
    AsNeeded,
}

impl Priority {
    pub(super) fn wire(self) -> &'static [u8] {
        match self {
            Self::Routine => b"R",
            Self::Stat => b"S",
            Self::Asap => b"A",
            Self::Preop => b"P",
            Self::Callback => b"C",
            Self::TimingCritical => b"T",
            Self::AsNeeded => b"PRN",
        }
    }
}

/// Caller-supplied non-repeating service window. No deadline or schedule runs here.
#[derive(Debug, Clone, Copy)]
pub struct Timing<'a> {
    /// Earliest service time, if the caller knows it.
    pub start: Option<Timestamp<'a>>,
    /// Latest service time, if supplied; the host resolves timezone/precision comparisons.
    pub end: Option<Timestamp<'a>>,
}

/// Explicit specimen source and configured sample identity placement.
#[derive(Debug, Clone, Copy)]
pub struct Specimen<'a> {
    /// Literal sample identity.
    pub sample: SampleId<'a>,
    /// ORM-only placer-field mapping. OML uses its standard SPM-2 identifier.
    pub field: SampleField,
    /// Source/type code from the caller, not inferred from a test.
    pub source: Code<'a>,
}

/// One caller-authorized new order or cancellation request.
#[derive(Debug, Clone, Copy)]
pub enum Order<'a> {
    /// New work: one test/battery under one placer identity. Replicates need new IDs.
    New {
        /// Stable caller-assigned placer order identity.
        placer: OrderId<'a>,
        /// Existing filler identity if supplied by the filler; never generated here.
        filler: Option<OrderId<'a>>,
        /// Requested test or battery with coding system.
        test: Code<'a>,
        /// Explicit specimen source and sample placement.
        specimen: Specimen<'a>,
        /// Requested priority.
        priority: Priority,
        /// Requested non-repeating timing window.
        timing: Timing<'a>,
    },
    /// Request cancellation; rendering this variant never establishes cancellation.
    Cancel {
        /// Stable original placer identity.
        placer: OrderId<'a>,
        /// Filler's identity if known.
        filler: Option<OrderId<'a>>,
    },
}

/// Caller-supplied PID content for patient-related new orders.
#[derive(Debug, Clone, Copy)]
pub struct Patient<'a> {
    /// Literal patient identifier, supplied by the host.
    pub identifier: &'a [u8],
    /// Explicit assigning namespace for the CX identifier.
    pub namespace: &'a [u8],
    /// Literal family name; the host supplies unavailable data explicitly.
    pub family_name: &'a [u8],
}

/// Whether this exchange's new orders are patient-related; no implicit default.
#[derive(Debug, Clone, Copy)]
pub enum Subject<'a> {
    /// Patient identification is required and rendered once for this message.
    Patient(Patient<'a>),
    /// Explicitly nonpatient work; no PID is synthesized.
    NonPatient,
}

/// MSH-11 processing environment, explicitly selected by the host.
#[derive(Debug, Clone, Copy)]
pub enum Processing {
    /// Production processing.
    Production,
    /// Training processing.
    Training,
    /// Debugging processing.
    Debugging,
}

impl Processing {
    pub(super) fn wire(self) -> &'static [u8] {
        match self {
            Self::Production => b"P",
            Self::Training => b"T",
            Self::Debugging => b"D",
        }
    }
}

/// Initiating message metadata supplied by the host, separate from order identities.
#[derive(Debug, Clone, Copy)]
pub struct Header<'a> {
    /// Sending application identifier.
    pub application: &'a [u8],
    /// Sending facility identifier.
    pub facility: &'a [u8],
    /// Receiving application identifier.
    pub peer_application: &'a [u8],
    /// Receiving facility identifier.
    pub peer_facility: &'a [u8],
    /// Stable message identity for this transmission; retries remain a host concern.
    pub control_id: crate::ControlId<'a>,
    /// Creation timestamp supplied by the host.
    pub timestamp: Timestamp<'a>,
    /// Edition to render.
    pub version: crate::Version,
    /// Processing environment; no production default is inferred.
    pub processing: Processing,
}
