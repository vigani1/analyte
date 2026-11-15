use super::{Error, Query, content, raw};
use crate::{AckCode, AckError, Message, Msh, Options, Response, Responsibility};

/// Host's authoritative cache outcome; unavailable is never an empty answer.
#[derive(Debug, Clone, Copy)]
pub enum CacheAnswer<'a> {
    /// Current authorized work exists; send it in a separate LAB-28 exchange.
    WorkExists,

    /// A complete, fresh lookup established that no work exists.
    NoWork,

    /// Lookup failed; report the supplied location rather than absence of work.
    Unavailable(AckError<'a>),
}

/// Required next exchange after query processing; contains no automatic retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryFollowup {
    /// Host must prepare a LAB-28 with app-authorized work.
    Work,

    /// Host must prepare the restricted negative LAB-28 for the same query scope.
    Negative,

    /// No work availability claim is possible.
    None,
}

/// Rendered functional query response and host's next action.
#[derive(Debug)]
pub struct QueryResponse {
    message: Message,
    followup: QueryFollowup,
}

impl QueryResponse {
    /// Returns the correlated RSP response, never a generic positive ACK.
    #[must_use]
    pub fn message(&self) -> &Message {
        &self.message
    }

    /// Returns the subsequent exchange required by the cache answer.
    #[must_use]
    pub fn followup(&self) -> QueryFollowup {
        self.followup
    }
}

/// Builds RSP with MSA/QAK correlation and an exact QPD echo (TF-2b §3.27.4).
///
/// The caller must establish durable responsibility before positive acceptance.
/// An unavailable lookup uses QAK AE and has no negative-work followup.
/// # Errors
/// Rejects unsafe response metadata, insufficient responsibility, error locations
/// or output beyond the caller's configured limits.
pub fn query_response(
    query: Query<'_>,
    answer: CacheAnswer<'_>,
    response: Response<'_>,
    responsibility: Responsibility,
    options: Options,
) -> Result<QueryResponse, Error> {
    let (code, status, error, followup) = match answer {
        CacheAnswer::WorkExists => (AckCode::Aa, b"OK".as_slice(), None, QueryFollowup::Work),
        CacheAnswer::NoWork => (AckCode::Aa, b"OK".as_slice(), None, QueryFollowup::Negative),
        CacheAnswer::Unavailable(error) => (
            AckCode::Ae,
            b"AE".as_slice(),
            Some(error),
            QueryFollowup::None,
        ),
    };
    // Reuse the core's responsibility, metadata, and ERR validation rather than
    // acquiring a second interpretation of when a positive acknowledgment is safe.
    let ack = crate::acknowledgment(
        query.message(),
        response,
        code,
        responsibility,
        error,
        options,
    )?
    .ok_or(content("MSH", 16))?;
    let msh = Msh::new(&ack)?;
    let source = msh.segment();
    let delimiters = ack.delimiters();
    let component = delimiters.component;
    let mut message_type = b"RSP".to_vec();
    message_type.push(component);
    message_type.extend_from_slice(b"K11");
    message_type.push(component);
    message_type.extend_from_slice(b"RSP_K11");
    let mut profile = b"LAB-27".to_vec();
    profile.push(component);
    profile.extend_from_slice(b"IHE");
    let mut wire = Wire::new(options.limits.message_bytes, delimiters.field);
    wire.append(b"MSH")?;
    for field in 2..=21 {
        let value = match field {
            9 => message_type.as_slice(),
            21 => profile.as_slice(),
            _ => raw(source, field),
        };
        wire.field(value)?;
    }
    wire.append(b"\r")?;
    for segment in ack.segments().skip(1) {
        wire.append(segment.raw())?;
        wire.append(b"\r")?;
    }
    wire.append(b"QAK")?;
    for value in [query.tag(), status, raw(query.qpd, 1)] {
        wire.field(value)?;
    }
    wire.append(b"\r")?;
    wire.append(query.qpd.raw())?;
    wire.append(b"\r")?;
    let message = Message::parse(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?;
    Ok(QueryResponse { message, followup })
}

/// Bounded output assembly shared by LAW renderers; check before allocating.
pub(super) struct Wire {
    pub(super) bytes: Vec<u8>,
    maximum: usize,
    separator: u8,
}

impl Wire {
    pub(super) fn new(maximum: usize, separator: u8) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            separator,
        }
    }

    pub(super) fn append(&mut self, value: &[u8]) -> Result<(), Error> {
        if self
            .bytes
            .len()
            .checked_add(value.len())
            .is_none_or(|size| size > self.maximum)
        {
            return Err(crate::Error::Limit {
                kind: "LAW output",
                segment: 0,
                field: 0,
            }
            .into());
        }
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    pub(super) fn field(&mut self, value: &[u8]) -> Result<(), Error> {
        self.append(&[self.separator])?;
        self.append(value)
    }
}
