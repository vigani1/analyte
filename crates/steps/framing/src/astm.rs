//! Frames ASTM data and checks checksums (LIS01-A2 6.3/8.3).
//! The dialog owns sequence acceptance, retransmissions, timers and durable-before-ACK rules.
use thiserror::Error;

/// Inbound record boundaries; checksums and envelopes are independent of this policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecordBoundaries {
    /// LIS01-A2 §§3.3.2, 6.3.1.1, 8.3.1.1: each record begins in a separate frame.
    #[default]
    Separate,
    /// ADR 0015: evidenced CR-ended records may share a verified inbound frame.
    Packed,
}
impl RecordBoundaries {
    /// Checks record endings after envelope verification, without changing source bytes.
    #[must_use]
    pub fn accepts(self, text: &[u8], final_frame: bool) -> bool {
        match self {
            Self::Separate => !text
                .iter()
                .position(|b| *b == CR)
                .is_some_and(|i| !final_frame || i.saturating_add(1) != text.len()),
            Self::Packed => !final_frame || text.last() == Some(&CR),
        }
    }
}

/// Start of frame.
pub const STX: u8 = 2;
/// Final frame terminator.
pub const ETX: u8 = 3;
/// End of transfer.
pub const EOT: u8 = 4;
/// Establishment request.
pub const ENQ: u8 = 5;
/// Positive acknowledgment.
pub const ACK: u8 = 6;
/// Negative acknowledgment.
pub const NAK: u8 = 21;
/// Intermediate frame terminator.
pub const ETB: u8 = 23;
/// Carriage return.
pub const CR: u8 = 13;
/// Line feed.
pub const LF: u8 = 10;

/// Valid ASCII frame digit; does not imply dialog sequence acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameNumber(u8);
impl FrameNumber {
    /// First frame of a transfer, per LIS01-A2 §6.3.2.
    pub const FIRST: Self = Self(b'1');
    /// Highest frame number before rollover (LIS01-A2 section 6.3.2).
    pub const MAX: Self = Self(b'7');
    /// Parses an ASCII digit 0 through 7.
    /// # Errors
    /// Returns `BadFrameNumber` for any other byte.
    pub fn new(byte: u8) -> Result<Self, Error> {
        if (b'0'..=Self::MAX.0).contains(&byte) {
            Ok(Self(byte))
        } else {
            Err(Error::BadFrameNumber(byte))
        }
    }

    /// Returns the exact ASCII digit.
    #[must_use]
    pub const fn ascii(self) -> u8 {
        self.0
    }

    /// Borrows the checked numeric sequence without its ASCII encoding.
    #[must_use]
    pub const fn value(self) -> u8 {
        // Constructors and rollover retain ASCII digits, so this explicit offset cannot wrap.
        self.0.wrapping_sub(b'0')
    }

    /// Advances a new frame, rolling seven over to zero.
    #[must_use]
    pub fn next(self) -> Self {
        Self(if self.0 == Self::MAX.0 {
            b'0'
        } else {
            self.0.wrapping_add(1)
        })
    }
}

/// Checksum compatibility selected explicitly by machine configuration.
#[derive(Debug, Clone, Copy, Default)]
pub enum HexPolicy {
    /// Require uppercase checksum letters.
    #[default]
    Strict,
    /// Also accept lowercase checksum letters, preserving original bytes.
    AcceptLowercase,
}

/// Valid total frame bound, including seven overhead bytes.
#[derive(Debug, Clone, Copy)]
pub struct Limit(usize);
impl Default for Limit {
    fn default() -> Self {
        Self(64_000)
    }
}

impl Limit {
    /// Maximum total frame size, including the envelope.
    #[must_use]
    pub const fn total_bytes(self) -> usize {
        self.0
    }

    /// Accepts total limits 8 through 64,000 (at least one text byte).
    /// # Errors
    /// Rejects limits outside that range.
    pub fn new(total: usize) -> Result<Self, Error> {
        if (8..=64_000).contains(&total) {
            Ok(Self(total))
        } else {
            Err(Error::InvalidLimit(total))
        }
    }
}

/// Specific framing failures; no payload is included in display messages.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// LIS02 records must begin in separate LIS01 frames.
    #[error("multiple or prematurely terminated LIS02 records in frame")]
    RecordBoundary,
    /// Invalid configured total bound.
    #[error("invalid ASTM frame limit {0}")]
    InvalidLimit(usize),
    /// Invalid ASCII frame digit.
    #[error("invalid frame number byte {0:#04x}")]
    BadFrameNumber(u8),
    /// Wire frame exceeds the configured bound.
    #[error("frame too long")]
    TooLong,
    /// Restricted text byte under §§6.6/8.6.
    #[error("restricted text byte {0:#04x}")]
    ControlInText(u8),
    /// A link-control byte (ENQ, EOT, ACK or NAK) arrived inside a frame. The
    /// partial frame is abandoned; the dialog must still act on this control
    /// byte as received (for example EOT ends the transfer, §§6.4/8.4).
    #[error("link control byte {0:#04x} inside a frame")]
    LinkControlInFrame(u8),
    /// Hexadecimal digit is lowercase in strict mode.
    #[error("lowercase checksum")]
    LowercaseChecksum,
    /// Checksum character is not hexadecimal.
    #[error("non-hexadecimal checksum byte {0:#04x}")]
    NonHexChecksum(u8),
    /// Computed and received checksums disagree.
    #[error("checksum mismatch: computed {computed:02X}, received {received:02X}")]
    BadChecksum {
        /// Raw-byte sum.
        computed: u8,
        /// Decoded wire checksum.
        received: u8,
    },
    /// Trailer requires CR then LF.
    #[error("missing trailer byte {0:#04x}")]
    MissingTrailer(u8),
    /// Next STX interrupted an unfinished frame.
    #[error("new STX before frame completion")]
    Interrupted,
    /// Connection ended or caller deadline expired mid-frame.
    #[error("incomplete frame")]
    Incomplete,
}

/// Complete validated frame with exact wire representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    raw: Vec<u8>,
    number: FrameNumber,
    text_end: usize,
    final_frame: bool,
}

impl Frame {
    /// Reports packed or prematurely ended records under the standard frame policy.
    #[must_use]
    pub fn has_packed_records(&self) -> bool {
        !RecordBoundaries::Separate.accepts(self.text(), self.is_final())
    }
    /// Reports lowercase hexadecimal digits from the verified checksum trailer.
    #[must_use]
    pub fn has_lowercase_checksum(&self) -> bool {
        self.raw
            .get(self.raw.len().saturating_sub(4)..self.raw.len().saturating_sub(2))
            .is_some_and(|digits| digits.iter().any(|b| matches!(*b, b'a'..=b'f')))
    }
    /// Returns record fragment spans relative to the original frame and their CR ending.
    /// The verified frame bounds every offset; partial records remain visible.
    pub fn record_fragments(&self) -> impl Iterator<Item = (std::ops::Range<usize>, bool)> + '_ {
        self.text()
            .split_inclusive(|b| *b == CR)
            .scan(2_usize, |offset, fragment| {
                // A verified frame caps the sum at its raw length.
                let end = offset.saturating_add(fragment.len());
                let range = *offset..end;
                *offset = end;
                Some((range, fragment.last() == Some(&CR)))
            })
    }

    /// Exact wire bytes, including checksum case and trailer.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Text bytes without decoding or normalization.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        self.raw.get(2..self.text_end).unwrap_or_default()
    }

    /// Valid frame digit; dialog must check expected sequence.
    #[must_use]
    pub const fn number(&self) -> FrameNumber {
        self.number
    }

    /// Whether ETX ended this frame.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.final_frame
    }
}

/// One bounded decoder output. Outside bytes remain available to the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Structurally valid, checksum-verified frame.
    Frame(Frame),
    /// Rejected frame through LF, or a bounded/interrupted prefix.
    Rejected {
        /// Specific cause.
        error: Error,
        /// Exact rejected bytes.
        raw: Vec<u8>,
    },
    /// Byte outside a frame (including dialog controls).
    Outside(u8),
}

#[derive(Debug)]
enum State {
    Idle,
    Number,
    Text,
    High,
    Low(u8),
    Cr(u8),
    Lf,
}

/// Incremental decoder retaining at most the configured total frame bound.
#[derive(Debug)]
pub struct Decoder {
    limit: Limit,
    policy: HexPolicy,
    state: State,
    raw: Vec<u8>,
    sum: u8,
    number: FrameNumber,
    text_end: usize,
    final_frame: bool,
    rejection: Option<Error>,
}

impl Decoder {
    /// Creates an empty decoder; no allocation depends on wire input.
    #[must_use]
    pub fn new(limit: Limit, policy: HexPolicy) -> Self {
        Self {
            limit,
            policy,
            state: State::Idle,
            raw: Vec::new(),
            sum: 0,
            number: FrameNumber::FIRST,
            text_end: 0,
            final_frame: false,
            rejection: None,
        }
    }

    fn reject(&mut self, error: Error) -> Event {
        self.state = State::Idle;
        self.rejection = None;
        Event::Rejected {
            error,
            raw: std::mem::take(&mut self.raw),
        }
    }

    /// Returns retained partial bytes for inspection/journaling.
    #[must_use]
    pub fn pending(&self) -> &[u8] {
        &self.raw
    }

    /// Ends a connection or deadline, returning incomplete evidence and resetting.
    pub fn finish(&mut self) -> Option<Event> {
        if self.raw.is_empty() {
            None
        } else {
            let error = self.rejection.take().unwrap_or(Error::Incomplete);
            Some(self.reject(error))
        }
    }

    /// Consumes one byte. Resumes framing at the next STX after any defect.
    /// An interrupting STX belongs to the new pending frame, not the rejection.
    pub fn push(&mut self, byte: u8) -> Option<Event> {
        if byte == STX {
            let event = self.finish().map(|event| match event {
                Event::Rejected { error, raw } => Event::Rejected {
                    error: if error == Error::Incomplete {
                        Error::Interrupted
                    } else {
                        error
                    },
                    raw,
                },
                Event::Frame(frame) => Event::Frame(frame),
                Event::Outside(b) => Event::Outside(b),
            });
            self.raw.push(byte);
            self.sum = 0;
            self.state = State::Number;
            return event;
        }
        if matches!(self.state, State::Idle) {
            return Some(Event::Outside(byte));
        }
        if is_link_control(byte) {
            self.raw.push(byte);
            return Some(self.reject(Error::LinkControlInFrame(byte)));
        }
        self.raw.push(byte);
        if self.rejection.is_some() {
            if byte == LF {
                let error = self.rejection.take().unwrap_or(Error::Incomplete);
                return Some(self.reject(error));
            }
            if self.raw.len() == self.limit.0 {
                return Some(self.reject(Error::TooLong));
            }
            return None;
        }
        let result = match self.state {
            State::Idle => return Some(Event::Outside(byte)),
            State::Number => FrameNumber::new(byte).map(|number| {
                self.number = number;
                self.sum = byte;
                self.state = State::Text;
            }),
            State::Text => {
                if byte == ETB || byte == ETX {
                    self.text_end = self.raw.len().saturating_sub(1);
                    self.final_frame = byte == ETX;
                    self.sum = self.sum.wrapping_add(byte);
                    self.state = State::High;
                    Ok(())
                } else if restricted(byte) {
                    Err(Error::ControlInText(byte))
                } else {
                    self.sum = self.sum.wrapping_add(byte);
                    Ok(())
                }
            }
            State::High => hex(byte, self.policy).map(|high| self.state = State::Low(high)),
            State::Low(high) => hex(byte, self.policy)
                .map(|low| self.state = State::Cr(high.wrapping_mul(16) | low)),
            State::Cr(received) => {
                if byte != CR {
                    Err(Error::MissingTrailer(CR))
                } else if received != self.sum {
                    Err(Error::BadChecksum {
                        computed: self.sum,
                        received,
                    })
                } else {
                    self.state = State::Lf;
                    Ok(())
                }
            }
            State::Lf => {
                if byte != LF {
                    Err(Error::MissingTrailer(LF))
                } else {
                    self.state = State::Idle;
                    return Some(Event::Frame(Frame {
                        raw: std::mem::take(&mut self.raw),
                        number: self.number,
                        text_end: self.text_end,
                        final_frame: self.final_frame,
                    }));
                }
            }
        };
        if self.raw.len() == self.limit.0 && byte != LF {
            return Some(self.reject(Error::TooLong));
        }
        if let Err(error) = result {
            if byte == LF {
                return Some(self.reject(error));
            }
            // Keep the original frame envelope, including the trailer, after a defect.
            self.rejection = Some(error);
        }
        None
    }
}

/// ENQ, EOT, ACK and NAK steer the link; they are never frame content.
fn is_link_control(b: u8) -> bool {
    matches!(b, ENQ | EOT | ACK | NAK)
}

fn restricted(b: u8) -> bool {
    matches!(b, 1..=6 | 10 | 16..=23)
}

fn hex(b: u8, policy: HexPolicy) -> Result<u8, Error> {
    match b {
        b'0'..=b'9' => Ok(b.wrapping_sub(b'0')),
        b'A'..=b'F' => Ok(b.wrapping_sub(b'A').wrapping_add(10)),
        b'a'..=b'f' => match policy {
            HexPolicy::Strict => Err(Error::LowercaseChecksum),
            HexPolicy::AcceptLowercase => Ok(b.wrapping_sub(b'a').wrapping_add(10)),
        },
        _ => Err(Error::NonHexChecksum(b)),
    }
}

/// Encodes one record lazily into bounded frames, advancing numbers only on output.
/// Does not concatenate records or interpret text encoding.
/// # Errors
/// Rejects restricted bytes before producing any frames.
pub fn encode(
    record: &[u8],
    first: FrameNumber,
    limit: Limit,
) -> Result<impl Iterator<Item = Vec<u8>> + '_, Error> {
    if let Some(byte) = record.iter().copied().find(|b| restricted(*b)) {
        return Err(Error::ControlInText(byte));
    }
    let mut number = first;
    let mut remaining = record;
    let mut done = false;
    Ok(std::iter::from_fn(move || {
        if done {
            return None;
        }
        let count = remaining.len().min(limit.0.saturating_sub(7));
        let (text, rest) = remaining.split_at(count);
        remaining = rest;
        done = rest.is_empty();
        let end = if done { ETX } else { ETB };
        let sum = text
            .iter()
            .fold(number.ascii(), |sum, b| sum.wrapping_add(*b))
            .wrapping_add(end);
        let mut raw = vec![STX, number.ascii()];
        raw.extend_from_slice(text);
        raw.push(end);
        raw.extend_from_slice(format!("{sum:02X}").as_bytes());
        raw.extend_from_slice(&[CR, LF]);
        number = number.next();
        Some(raw)
    }))
}

/// A retained transfer cannot be decoded as accepted data frames.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReceiptError {
    /// Incremental frame verification failed.
    #[error(transparent)]
    Frame(#[from] Error),
    /// A stored transfer contains non-frame bytes.
    #[error("receipt contains bytes outside ASTM frames")]
    Outside,
    /// Reassembled syntax exceeds its configured limit.
    #[error("ASTM assembled receipt exceeds syntax bound")]
    Limit,
    /// The last frame or transfer is unfinished.
    #[error("receipt contains incomplete ASTM transfer")]
    Incomplete,
}

/// Reassembles retained accepted frames using the live frame decoder.
/// Sequence admission and durable custody remain the live dialog's responsibility.
/// # Errors
/// Rejects framing defects, record boundaries, excessive assembly and incomplete transfers.
pub fn decode(
    raw: &[u8],
    limit: Limit,
    checksum: HexPolicy,
    boundaries: RecordBoundaries,
    maximum: usize,
) -> Result<Vec<u8>, ReceiptError> {
    let mut decoder = Decoder::new(limit, checksum);
    let mut payload = Vec::new();
    let mut final_frame = false;
    for byte in raw {
        if let Some(event) = decoder.push(*byte) {
            match event {
                Event::Frame(frame) => {
                    if !boundaries.accepts(frame.text(), frame.is_final()) {
                        return Err(Error::RecordBoundary.into());
                    }
                    if payload
                        .len()
                        .checked_add(frame.text().len())
                        .is_none_or(|n| n > maximum)
                    {
                        return Err(ReceiptError::Limit);
                    }
                    payload.extend_from_slice(frame.text());
                    final_frame = frame.is_final();
                }
                Event::Rejected { error, .. } => return Err(error.into()),
                Event::Outside(_) => return Err(ReceiptError::Outside),
            }
        }
    }
    if decoder.finish().is_some() || !final_frame {
        return Err(ReceiptError::Incomplete);
    }
    Ok(payload)
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    // CLSI LIS01-A2 §3.3.2, §6.3.1.1, §8.3.1.1
    #[test]
    fn record_fragment_spans_keep_original_bytes() -> Result<(), Error> {
        for text in [b"H|x\r".as_slice(), b"H|x\rP|1\r", b"H|x", b"H|x\rP|"] {
            let wires = encode(text, FrameNumber::FIRST, Limit::default())?;
            for wire in wires {
                let mut decoder = Decoder::new(Limit::default(), HexPolicy::Strict);
                let frame = wire
                    .iter()
                    .filter_map(|b| decoder.push(*b))
                    .find_map(|e| match e {
                        Event::Frame(frame) => Some(frame),
                        Event::Outside(_) | Event::Rejected { .. } => None,
                    })
                    .ok_or(Error::RecordBoundary)?;
                let fragments = frame.record_fragments().collect::<Vec<_>>();
                let recovered = fragments
                    .iter()
                    .flat_map(|(span, _)| frame.raw().get(span.clone()).unwrap_or_default())
                    .copied()
                    .collect::<Vec<_>>();
                assert_eq!(recovered, text);
                let crs = text.iter().filter(|b| **b == CR).count();
                assert_eq!(
                    frame.has_packed_records(),
                    crs > 1 || (crs > 0 && (!frame.is_final() || text.last() != Some(&CR)))
                );
                for (span, ended) in fragments {
                    assert_eq!(
                        ended,
                        frame.raw().get(span).is_some_and(|s| s.last() == Some(&CR))
                    );
                }
                assert!(!frame.has_lowercase_checksum());
            }
        }
        Ok(())
    }
}
