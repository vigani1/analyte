//! Frames HL7 messages using single-byte MLLP Release 1 delimiters.
use thiserror::Error;

/// Start block byte.
pub const SB: u8 = 11;
/// End block byte.
pub const EB: u8 = 28;
/// Required trailer byte.
pub const CR: u8 = 13;
/// Specific wire or configuration defect.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Configured payload bound must leave room for three overhead bytes.
    #[error("invalid message bound")]
    InvalidLimit,
    /// Byte arrived without a block.
    #[error("byte outside block")]
    Outside,
    /// End marker arrived without a block.
    #[error("end without start")]
    EndWithoutStart,
    /// Start marker interrupted a block.
    #[error("new start before end")]
    Interrupted,
    /// End marker was not followed by CR.
    #[error("missing CR trailer")]
    MissingCr,
    /// Payload exceeded configured maximum.
    #[error("message too long")]
    TooLong,
    /// Byte is outside the repertoire of §1.2.
    #[error("invalid payload byte {0:#04x}")]
    InvalidByte(u8),
    /// Caller ended a partial block.
    #[error("incomplete block")]
    Incomplete,
}

/// Complete block or bounded rejected evidence; payloads must not be logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Exact block bytes, including SB, EB and CR.
    Block(Vec<u8>),
    /// Exact rejected bytes and specific cause.
    Rejected {
        /// Cause, without payload in its display.
        error: Error,
        /// Rejected evidence; restarting SB belongs to new pending block.
        raw: Vec<u8>,
    },
}

/// Valid maximum payload size, excluding three framing bytes.
#[derive(Debug, Clone, Copy)]
pub struct Limit(usize);
impl Limit {
    /// Validates a deployment-selected bound (no protocol default exists).
    /// # Errors
    /// Rejects values larger than `isize::MAX - 4`.
    pub fn new(max: usize) -> Result<Self, Error> {
        if max <= (isize::MAX as usize).saturating_sub(4) {
            Ok(Self(max))
        } else {
            Err(Error::InvalidLimit)
        }
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Text,
    Trailer,
}

/// Incremental block decoder bounded by payload limit plus four evidence bytes.
#[derive(Debug)]
pub struct Decoder {
    limit: Limit,
    state: State,
    raw: Vec<u8>,
    count: usize,
}

impl Decoder {
    /// Creates an idle decoder without allocating from input.
    #[must_use]
    pub fn new(limit: Limit) -> Self {
        Self {
            limit,
            state: State::Idle,
            raw: Vec::new(),
            count: 0,
        }
    }

    /// Exact partial block; callers must journal bytes before feeding them.
    #[must_use]
    pub fn pending(&self) -> &[u8] {
        &self.raw
    }

    fn reject(&mut self, error: Error) -> Event {
        self.state = State::Idle;
        Event::Rejected {
            error,
            raw: std::mem::take(&mut self.raw),
        }
    }

    /// Finishes a connection or deadline and returns incomplete evidence.
    pub fn finish(&mut self) -> Option<Event> {
        if self.raw.is_empty() {
            None
        } else {
            Some(self.reject(Error::Incomplete))
        }
    }

    /// Consumes a byte. Outside bytes each produce an error; new SB restarts;
    /// all other defects return to idle. No bytes disappear silently.
    pub fn push(&mut self, byte: u8) -> Option<Event> {
        if byte == SB {
            let event = if self.raw.is_empty() {
                None
            } else {
                Some(self.reject(Error::Interrupted))
            };
            self.raw.push(byte);
            self.count = 0;
            self.state = State::Text;
            return event;
        }
        self.raw.push(byte);
        match self.state {
            State::Idle => Some(self.reject(if byte == EB {
                Error::EndWithoutStart
            } else {
                Error::Outside
            })),
            State::Text => {
                if byte == EB {
                    self.state = State::Trailer;
                    None
                } else if !valid(byte) {
                    Some(self.reject(Error::InvalidByte(byte)))
                } else if self.count == self.limit.0 {
                    Some(self.reject(Error::TooLong))
                } else {
                    self.count = self.count.saturating_add(1);
                    None
                }
            }
            State::Trailer => {
                if byte == CR {
                    self.state = State::Idle;
                    Some(Event::Block(std::mem::take(&mut self.raw)))
                } else {
                    Some(self.reject(Error::MissingCr))
                }
            }
        }
    }
}

fn valid(byte: u8) -> bool {
    byte > 31 || byte == CR
}

/// Borrows the content inside one MLLP Release 1 §1.2 block envelope.
/// Returns `None` if SB or the EB/CR trailer is missing. This does not validate
/// the payload repertoire or size; use [`Decoder`] before interpreting input.
#[must_use]
pub fn payload(raw: &[u8]) -> Option<&[u8]> {
    raw.strip_prefix(&[SB])?.strip_suffix(&[EB, CR])
}

/// Encodes permitted §1.2 payload bytes, without character conversion.
/// # Errors
/// Rejects an oversized payload or a forbidden control byte before allocation.
pub fn encode(payload: &[u8], limit: Limit) -> Result<Vec<u8>, Error> {
    if payload.len() > limit.0 {
        return Err(Error::TooLong);
    }
    if let Some(byte) = payload.iter().copied().find(|b| !valid(*b)) {
        return Err(Error::InvalidByte(byte));
    }
    let mut raw = vec![SB];
    raw.extend_from_slice(payload);
    raw.extend_from_slice(&[EB, CR]);
    Ok(raw)
}

/// A retained receipt must contain exactly one complete MLLP block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReceiptError {
    /// The live decoder refused the envelope.
    #[error(transparent)]
    Frame(#[from] Error),
    /// Another block or bytes followed the first complete block.
    #[error("receipt contains multiple MLLP blocks or trailing bytes")]
    Trailing,
    /// The receipt did not reach its acceptance boundary.
    #[error("receipt contains incomplete MLLP block")]
    Incomplete,
}

/// Verifies one retained block with the live decoder and borrows its exact payload.
/// # Errors
/// Rejects incomplete, malformed, excessive or multiple blocks and trailing bytes.
pub fn decode(raw: &[u8], limit: Limit) -> Result<&[u8], ReceiptError> {
    let mut decoder = Decoder::new(limit);
    let mut complete = false;
    for byte in raw {
        if complete {
            return Err(ReceiptError::Trailing);
        }
        if let Some(event) = decoder.push(*byte) {
            match event {
                Event::Block(_) => complete = true,
                Event::Rejected { error, .. } => return Err(error.into()),
            }
        }
    }
    if !complete {
        return Err(ReceiptError::Incomplete);
    }
    payload(raw).ok_or(ReceiptError::Incomplete)
}

impl crate::Decode for Decoder {
    type Event = Event;
    type Error = std::convert::Infallible;
    fn push_byte(&mut self, byte: u8) -> Result<Option<Event>, Self::Error> {
        Ok(self.push(byte))
    }
    fn finish_stream(&mut self) -> Result<Option<Event>, Self::Error> {
        Ok(self.finish())
    }
}
