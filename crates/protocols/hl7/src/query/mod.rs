//! Reads legacy HL7 display queries with documented evidence (2.3.1 section 2.18 and 2.5.1 section 5.10).
//!
//! These pure views do not schedule responses or access the host's order cache.
//! Unsupported filters remain available to the host; they must never be ignored.
use crate::{AckCode, Field, Message, Msa, Msh, Segment};

mod specimen;
mod validation;
pub use crate::encode::query::{CacheAnswer, acknowledgment, display};
pub use specimen::{SpecimenSelector, specimen};

pub(crate) fn invalid(segment: &'static str, field: usize) -> crate::Error {
    crate::Error::Control { segment, field }
}

pub(crate) fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}

/// A validated legacy deferred display query, retaining every original filter.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub(crate) message: &'a Message,
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
        let msh = Msh::new(message)?;
        if let Some(section) = msh.version().legacy_query_withdrawal() {
            return Err(crate::Error::WithdrawnQuery {
                edition: msh.version().as_str(),
                section,
            });
        }
        let mut kind = msh.message_type();
        if kind.next() != Some(b"QRY")
            || kind.next() != Some(b"Q02")
            || kind.next().is_some_and(|v| v != b"QRY_Q02")
        {
            return Err(invalid("MSH", 9));
        }
        let mut segments = message.segments().skip(1).peekable();
        if msh.version().legacy_query_structure() {
            while segments.peek().is_some_and(|s| s.id() == b"SFT") {
                segments.next();
            }
        }
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
        validation::definition(definition, msh.version())?;
        if let Some(filter) = filter {
            validation::filter(filter, msh.version())?;
        }
        // Other valid table entries require different response transactions.
        // This API only renders deferred display replies, not record/tabular ones.
        if raw(definition, 2) != b"D" {
            return Err(invalid("QRD", 2));
        }
        if raw(definition, 3) != b"D" {
            return Err(invalid("QRD", 3));
        }
        Ok(Self {
            message,
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
        let msh = Msh::new(message)?;
        if let Some(section) = msh.version().legacy_query_withdrawal() {
            return Err(crate::Error::WithdrawnQuery {
                edition: msh.version().as_str(),
                section,
            });
        }
        let mut kind = msh.message_type();
        if kind.next() != Some(b"ACK")
            || kind.next() != Some(b"Q03")
            || kind.next().is_some_and(|v| v != b"ACK")
        {
            return Err(invalid("MSH", 9));
        }
        let mut segments = message.segments().skip(1).peekable();
        if msh.version().legacy_query_structure() {
            while segments.peek().is_some_and(|s| s.id() == b"SFT") {
                segments.next();
            }
        }
        let msa = Msa::for_version(segments.next().ok_or(invalid("MSA", 0))?, msh.version())?;
        if msa.control_id() != expected.raw() {
            return Err(invalid("MSA", 2));
        }
        if matches!(msa.code(), AckCode::Ca | AckCode::Ce | AckCode::Cr) {
            return Err(invalid("MSA", 1));
        }
        for segment in segments {
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
