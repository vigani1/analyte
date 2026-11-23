//! Reads legacy HL7 display queries with documented evidence (2.3.1 section 2.18 and 2.5.1 section 5.10).
//!
//! These pure views do not schedule responses or access the host's order cache.
//! Unsupported filters remain available to the host; they must never be ignored.
use crate::{AckCode, Field, Message, Msa, Msh, Segment};

mod specimen;
mod validation;
pub use crate::encode::query::{CacheAnswer, acknowledgment, display, record_display};
pub use specimen::{SpecimenSelector, record_specimen, specimen};

pub(crate) fn invalid(segment: &'static str, field: usize) -> crate::Error {
    crate::Error::Control { segment, field }
}

pub(crate) fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}

/// Checks the header rules shared by every legacy query exchange message: the edition
/// still defines original queries (2.8.2 §5.10.2 and 2.9.1 §5.9.2 withdraw them), MSH-9 is the
/// expected code, event and optional structure, and the edition's SFT segments after
/// MSH are skipped. Returns the header and the segments that follow.
fn legacy_header<'a>(
    message: &'a Message,
    [code, event, structure]: [&[u8]; 3],
) -> Result<
    (
        Msh<'a>,
        std::iter::Peekable<impl Iterator<Item = Segment<'a>>>,
    ),
    crate::Error,
> {
    let msh = Msh::new(message)?;
    if let Some(section) = msh.version().legacy_query_withdrawal() {
        return Err(crate::Error::WithdrawnQuery {
            edition: msh.version().as_str(),
            section,
        });
    }
    let mut kind = msh.message_type();
    if kind.next() != Some(code)
        || kind.next() != Some(event)
        || kind.next().is_some_and(|v| v != structure)
    {
        return Err(invalid("MSH", 9));
    }
    let mut segments = message.segments().skip(1).peekable();
    if msh.version().legacy_query_structure() {
        while segments.peek().is_some_and(|s| s.id() == b"SFT") {
            segments.next();
        }
    }
    Ok((msh, segments))
}

/// The supported legacy query agreements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Standard deferred display with a line limit.
    Display,
    /// Documented HL7 2.3.1 record-oriented query with positional display replies.
    RecordDisplay,
}

/// A validated legacy deferred display query, retaining every original filter.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub(crate) message: &'a Message,
    pub(crate) profile: Profile,
    pub(crate) definition: Segment<'a>,
    pub(crate) filter: Option<Segment<'a>>,
}

impl<'a> Request<'a> {
    /// Checks the supported single-query, non-continuation display structure.
    ///
    /// # Errors
    /// Returns the offending segment and field for missing or unsupported content.
    /// QRD/QRF validation preserves local filter values; the host must interpret
    /// every filter before claiming that a cache answer is trusted.
    pub fn new(message: &'a Message) -> Result<Self, crate::Error> {
        Self::with_profile(message, Profile::Display)
    }

    /// Reads the selected legacy query agreement without changing standard defaults.
    ///
    /// # Errors
    /// Refuses messages outside the agreement's structure, fields or edition.
    pub fn with_profile(message: &'a Message, profile: Profile) -> Result<Self, crate::Error> {
        let (msh, mut segments) = legacy_header(message, [b"QRY", b"Q02", b"QRY_Q02"])?;
        let definition = segments
            .next()
            .filter(|s| s.id() == b"QRD")
            .ok_or(invalid("QRD", 0))?;
        let filter = if segments.peek().is_some_and(|s| s.id() == b"QRF") {
            segments.next()
        } else {
            None
        };
        if let Some(extra) = segments.next() {
            // Continuation needs a host-owned lifecycle; never process a partial lookup.
            let name = match extra.id() {
                b"DSC" => "DSC",
                b"ERQ" => "ERQ",
                b"QRD" => "QRD",
                b"QRF" => "QRF",
                _ => "QRY",
            };
            return Err(invalid(name, 0));
        }
        match profile {
            Profile::Display => validation::definition(definition, msh.version())?,
            Profile::RecordDisplay => validation::record_definition(definition, msh.version())?,
        }
        if let Some(filter) = filter {
            validation::filter(filter, msh.version())?;
        }
        // Other valid table entries require different response transactions.
        // This API only renders deferred display replies, not record/tabular ones.
        if profile == Profile::Display && raw(definition, 2) != b"D" {
            return Err(invalid("QRD", 2));
        }
        if raw(definition, 3) != b"D" {
            return Err(invalid("QRD", 3));
        }
        Ok(Self {
            message,
            profile,
            definition,
            filter,
        })
    }

    /// Returns QRD-4 unchanged, independently of message control IDs.
    #[must_use]
    pub fn query_id(self) -> &'a [u8] {
        raw(self.definition, 4)
    }

    /// Returns QRD including repeated who/what filters and the quantity limit.
    #[must_use]
    pub fn definition(self) -> Segment<'a> {
        self.definition
    }

    /// Returns QRF including where, when and locally agreed qualifiers.
    #[must_use]
    pub fn filter(self) -> Option<Segment<'a>> {
        self.filter
    }

    /// Returns the original message for protected retention.
    #[must_use]
    pub fn message(self) -> &'a Message {
        self.message
    }
}

/// Message-level acknowledgment of a DSR, never an order acceptance.
#[derive(Debug, Clone, Copy)]
pub struct DisplayAck<'a>(Msa<'a>);

impl<'a> DisplayAck<'a> {
    /// Checks ACK/Q03 structure and correlation against the actual DSR ID.
    ///
    /// # Errors
    /// Rejects mismatched correlation, invalid acknowledgments or extra segments.
    pub fn new(message: &'a Message, expected: crate::ControlId<'_>) -> Result<Self, crate::Error> {
        Self::with_profile(message, expected, Profile::Display)
    }

    /// Checks a receipt acknowledgment using the selected legacy agreement.
    ///
    /// # Errors
    /// Refuses invalid or uncorrelated acknowledgment messages.
    pub fn with_profile(
        message: &'a Message,
        expected: crate::ControlId<'_>,
        profile: Profile,
    ) -> Result<Self, crate::Error> {
        let (msh, mut segments) = legacy_header(message, [b"ACK", b"Q03", b"ACK"])?;
        let msa = Msa::for_version(segments.next().ok_or(invalid("MSA", 0))?, msh.version())?;
        if msa.control_id() != expected.raw() {
            return Err(invalid("MSA", 2));
        }
        if matches!(msa.code(), AckCode::Ca | AckCode::Ce | AckCode::Cr) {
            return Err(invalid("MSA", 1));
        }
        for segment in segments {
            // YHLO LIS V1.1 sections 2.3.8 and 2.4.2 use a numeric ERR-1.
            if profile == Profile::RecordDisplay
                && segment.id() == b"ERR"
                && segment.fields().skip(1).all(|field| field.raw().is_empty())
                && raw(segment, 1) == b"0"
                && msa.code() == AckCode::Aa
            {
                continue;
            }
            crate::ErrView::new(segment, msh.version())?;
        }
        Ok(Self(msa))
    }

    /// Returns application receipt status; it does not change any order lifecycle.
    #[must_use]
    pub fn code(self) -> AckCode {
        self.0.code()
    }
}
