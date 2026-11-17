//! Builds LAW RSP^K11 query responses and names the host's next exchange.
use super::append_ack_segments;
use crate::encode::wire::Wire;
use crate::law::{Error, Query, content, raw};
use crate::{AckCode, AckError, Message, Options, Response, Responsibility};

/// Host's authoritative cache outcome; unavailable is never an empty answer.
#[derive(Debug, Clone, Copy)]
pub enum CacheAnswer<'a> {
    /// Current authorized work exists; send it in a separate LAB-28 exchange.
    WorkExists,

    /// A complete, fresh lookup established that no work exists.
    NoWork,

    /// Lookup failed; report the supplied location rather than absence of work.
    Unavailable(AckError<'a>),
    /// A syntactically valid query is outside the manager's configured support (B.7.7).
    Rejected(AckError<'a>),
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
    if !crate::law::profile::timestamp(response.timestamp, true) {
        return Err(content("MSH", 7));
    }
    let (code, status, error, followup) = match answer {
        CacheAnswer::WorkExists => (AckCode::Aa, b"OK".as_slice(), None, QueryFollowup::Work),
        CacheAnswer::NoWork => (AckCode::Aa, b"OK".as_slice(), None, QueryFollowup::Negative),
        CacheAnswer::Unavailable(error) => (
            AckCode::Ae,
            b"AE".as_slice(),
            Some(error),
            QueryFollowup::None,
        ),
        CacheAnswer::Rejected(error) => (
            AckCode::Ar,
            b"AR".as_slice(),
            Some(error),
            QueryFollowup::None,
        ),
    };
    // Reuse the core's responsibility, metadata, and ERR validation rather than
    // acquiring a second interpretation of when a positive acknowledgment is safe.
    let ack = crate::encode::ack::acknowledgment_with_msh(
        crate::law::msh(query.message())?,
        response,
        code,
        responsibility,
        error,
        options,
        crate::encode::ack::AckProfile::Law,
    )?
    .ok_or(content("MSH", 16))?;
    let source = ack.segments().next().ok_or(content("MSH", 0))?;
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
            _ => crate::law::envelope::reply_field(source, field),
        };
        wire.field(value)?;
    }
    wire.append(b"\r")?;
    append_ack_segments(&mut wire, &ack)?;
    wire.append(b"QAK")?;
    for value in [query.tag(), status, raw(query.qpd, 1)] {
        wire.field(value)?;
    }
    wire.append(b"\r")?;
    wire.append(query.qpd.raw())?;
    wire.append(b"\r")?;
    let message = crate::law::parse_message(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?;
    crate::law::validate_control(&message, crate::law::ControlDirection::Send)?;
    Ok(QueryResponse { message, followup })
}
