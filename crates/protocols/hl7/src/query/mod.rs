//! Reads legacy HL7 display queries with documented evidence (2.3.1 section 2.18 and 2.5.1 section 5.10).
//!
//! These pure views do not schedule responses or access the host's order cache.
//! Unsupported filters remain available to the host; they must never be ignored.
use crate::{AckCode, Field, Message, Msa, Msh, Segment, Version};

mod specimen;
mod validation;
pub use specimen::{SpecimenSelector, specimen};

fn invalid(segment: &'static str, field: usize) -> crate::Error {
    crate::Error::Control { segment, field }
}

fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}

/// A validated legacy deferred display query, retaining every original filter.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    message: &'a Message,
    definition: Segment<'a>,
    filter: Option<Segment<'a>>,
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

/// Trusted cache result; lookup failure has no successful empty representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAnswer {
    /// Trusted lookup found matching data.
    Found,
    /// Trusted lookup found no matching data.
    Empty,
    /// Lookup could not be trusted, including an unavailable lab app.
    Unavailable,
}

/// Builds QCK/Q02 after the host durably stores the QRY, with explicit cache status.
///
/// # Errors
/// Rejects unsafe identity/time fields, enhanced acknowledgment negotiation and limits.
pub fn acknowledgment(
    request: Request<'_>,
    response: crate::Response<'_>,
    answer: CacheAnswer,
    options: crate::Options,
) -> Result<Message, crate::Error> {
    let (code, status, responsibility) = match answer {
        CacheAnswer::Found => (
            AckCode::Aa,
            b"OK".as_slice(),
            crate::Responsibility::StoredAndInterpreted,
        ),
        CacheAnswer::Empty => (
            AckCode::Aa,
            b"NF".as_slice(),
            crate::Responsibility::StoredAndInterpreted,
        ),
        CacheAnswer::Unavailable => (AckCode::Ae, b"AE".as_slice(), crate::Responsibility::Stored),
    };
    let msh = Msh::new(request.message)?;
    if msh.mode() != crate::AckMode::Original {
        return Err(crate::Error::AckMode);
    }
    let ack = crate::acknowledgment(
        request.message,
        response,
        code,
        responsibility,
        None,
        options,
    )?
    .ok_or(crate::Error::AckMode)?;
    let d = request.message.delimiters();
    let mut output = Vec::new();
    encode_response_segments(
        &mut output,
        &ack,
        msh.version(),
        (b"QCK", b"Q02", b"QCK_Q02"),
    );
    output.extend_from_slice(b"QAK");
    // Original-style queries have no QAK tag (2.3.1 §2.24.22.1).
    output.push(d.field);
    output.push(d.field);
    output.extend_from_slice(status);
    output.push(b'\r');
    Message::parse(&output, options)
}

/// Builds a complete DSR/Q03 for a trusted lookup with display lines.
///
/// The caller supplies presentation text under its configured interface. This
/// function assigns no laboratory meaning to a line position. NF and AE replies
/// terminate at QCK; they cannot accidentally produce an empty positive DSR.
///
/// # Errors
/// Rejects empty data, unsupported quantity units, limits and delimiter injection.
pub fn display(
    request: Request<'_>,
    response: crate::Response<'_>,
    qck_id: crate::ControlId<'_>,
    lines: &[&[u8]],
    options: crate::Options,
) -> Result<Message, crate::Error> {
    if response.control_id == qck_id {
        return Err(invalid("MSH", 10));
    }
    if lines.is_empty() || lines.len() > options.limits.segments {
        return Err(invalid("DSP", 0));
    }
    let d = request.message.delimiters();
    let mut quantity = raw(request.definition, 7).split(|b| *b == d.component);
    let amount = quantity.next().unwrap_or_default();
    let maximum = amount
        .iter()
        .try_fold(0_usize, |value, digit| {
            value
                .checked_mul(10)?
                .checked_add(usize::from(digit.checked_sub(b'0')?))
        })
        .ok_or(invalid("QRD", 7))?;
    // Page, record and local units require an interface-specific pagination contract.
    if !matches!(quantity.next(), None | Some(b"" | b"LI")) || lines.len() > maximum {
        return Err(invalid("QRD", 7));
    }
    let qck = acknowledgment(request, response, CacheAnswer::Found, options)?;
    let mut output = Vec::new();
    encode_response_segments(
        &mut output,
        &qck,
        Msh::new(request.message)?.version(),
        (b"DSR", b"Q03", b"DSR_Q03"),
    );
    output.extend_from_slice(request.definition.raw());
    output.push(b'\r');
    if let Some(filter) = request.filter {
        output.extend_from_slice(filter.raw());
        output.push(b'\r');
    }
    for (index, line) in lines.iter().enumerate() {
        if line.len() > 300
            || line
                .iter()
                .any(|b| !b.is_ascii() || b.is_ascii_control() || d.bytes().contains(b))
        {
            return Err(invalid("DSP", 3));
        }
        let number = index.checked_add(1).ok_or(invalid("DSP", 1))?;
        output.extend_from_slice(b"DSP");
        output.push(d.field);
        output.extend_from_slice(number.to_string().as_bytes());
        output.push(d.field);
        output.push(d.field);
        output.extend_from_slice(line);
        output.push(b'\r');
        if output.len() > options.limits.message_bytes {
            return Err(invalid("DSP", 0));
        }
    }
    Message::parse(&output, options)
}

// Both query replies retain the acknowledgment fields and replace only MSH-9.
fn encode_response_segments(
    output: &mut Vec<u8>,
    message: &Message,
    version: Version,
    kind: (&[u8], &[u8], &[u8]),
) {
    let (message_type, event, structure) = kind;
    let d = message.delimiters();
    for segment in message.segments() {
        if segment.id() == b"MSH" {
            output.extend_from_slice(b"MSH");
            for field in 2..=18 {
                output.push(d.field);
                if field == 9 {
                    output.extend_from_slice(message_type);
                    output.push(d.component);
                    output.extend_from_slice(event);
                    if version.legacy_query_structure() {
                        output.push(d.component);
                        output.extend_from_slice(structure);
                    }
                } else {
                    output.extend_from_slice(raw(segment, field));
                }
            }
        } else {
            output.extend_from_slice(segment.raw());
        }
        output.push(b'\r');
    }
}
