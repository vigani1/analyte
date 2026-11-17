//! Frames VIDAS messages incrementally (BCI Appendix A chapter 3).
use thiserror::Error;

/// Packet acknowledgment boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Acknowledge checksum before ETX.
    Literal,
    /// Acknowledge after ETX.
    Alternate,
}

/// Explicit installed checksum policy; no inferred default.
#[derive(Debug, Clone, Copy)]
pub struct Checksum {
    /// Include STX (notes specify false; site confirmation required).
    pub include_stx: bool,
    /// Include configured control suffix bytes in the sum.
    pub include_suffix: bool,
    /// Independent hex output and input options.
    pub hex: HexOptions,
}

/// Independent choices for checksum output spelling and accepted input spelling.
#[derive(Debug, Clone, Copy)]
pub struct HexOptions {
    /// Emit uppercase rather than lowercase hex.
    pub uppercase: bool,
    /// Accept either hex letter case on input.
    pub accept_either_case: bool,
}

/// Validated framing settings. No default configuration is provided.
#[derive(Debug, Clone)]
pub struct Config {
    mode: Mode,
    chunk: usize,
    maximum: usize,
    suffix: Vec<u8>,
    checksum: Checksum,
}

impl Config {
    /// Validates site-selected bounds and CR/LF suffix (applied to every control).
    /// # Errors
    /// Rejects zero or excessive bounds, and suffixes other than CR, LF or CRLF.
    pub fn new(
        mode: Mode,
        chunk: usize,
        maximum: usize,
        suffix: &[u8],
        checksum: Checksum,
    ) -> Result<Self, Error> {
        if chunk == 0
            || chunk > 80
            || maximum == 0
            || maximum > 1920
            || !matches!(suffix, [] | [CR] | [LF] | [CR, LF])
        {
            return Err(Error::Config);
        }
        Ok(Self {
            mode,
            chunk,
            maximum,
            suffix: suffix.to_vec(),
            checksum,
        })
    }

    /// Returns the configured acknowledgment boundary.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Encodes a control with the explicitly selected suffix.
    #[must_use]
    pub fn control(&self, byte: u8) -> Vec<u8> {
        let mut bytes = vec![byte];
        bytes.extend_from_slice(&self.suffix);
        bytes
    }

    /// Encodes through checksum for Literal, through ETX for Alternate.
    /// # Errors
    /// Rejects empty, nonprintable or excessive payloads.
    pub fn encode(&self, body: &[u8]) -> Result<Vec<u8>, Error> {
        if body.is_empty()
            || body.len() > self.maximum
            || !body.iter().all(|b| (32..=126).contains(b))
        {
            return Err(Error::Payload);
        }
        let mut raw = self.control(STX);
        for chunk in body.chunks(self.chunk) {
            raw.extend(self.control(RS));
            raw.extend_from_slice(chunk);
        }
        raw.extend(self.control(GS));
        let sum = checksum(&raw, self);
        raw.extend(hex(sum, self.checksum.hex.uppercase).bytes());
        if self.mode == Mode::Alternate {
            raw.extend(self.control(ETX));
        }
        Ok(raw)
    }
}

fn checksum(raw: &[u8], config: &Config) -> u8 {
    raw.iter()
        .skip(if config.checksum.include_stx {
            0
        } else {
            config.suffix.len().saturating_add(1)
        })
        .copied()
        .filter(|b| {
            (*b != STX || config.checksum.include_stx)
                && (config.checksum.include_suffix || !matches!(*b, LF | CR))
        })
        .fold(0u8, u8::wrapping_add)
}

fn hex(sum: u8, uppercase: bool) -> String {
    if uppercase {
        format!("{sum:02X}")
    } else {
        format!("{sum:02x}")
    }
}

/// A checksummed acceptance boundary; raw contains every byte through that boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// Original framing and checksum bytes.
    pub raw: Vec<u8>,
    /// Reassembled fields (RS can split a field).
    pub body: Vec<u8>,
}

/// Specific framing failure. The host's byte journal remains authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Invalid installed settings.
    #[error("invalid framing configuration")]
    Config,
    /// Payload violates printable ASCII or size bounds.
    #[error("invalid or excessive payload")]
    Payload,
    /// Unexpected byte or suffix.
    #[error("unexpected framing byte")]
    Sequence,
    /// Packet sum does not match.
    #[error("checksum mismatch")]
    Checksum,
    /// Stored receipt ended before an acceptance boundary.
    #[error("VIDAS receipt is incomplete")]
    Incomplete,
    /// Bytes followed the single stored packet.
    #[error("VIDAS receipt contains trailing bytes")]
    Trailing,
    /// Decoder is stopped after a failure.
    #[error("decoder stopped; retain journal and start a new session")]
    Stopped,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Start,
    Record,
    Text,
    HexFirst,
    HexLast(u8),
    End,
    Done,
    Failed,
}

/// One-packet bounded incremental decoder. After errors it stays stopped.
/// ETX/EOT following a Literal boundary belong to the dialog and byte journal.
pub struct Decoder {
    config: Config,
    state: State,
    raw: Vec<u8>,
    body: Vec<u8>,
    chunk: usize,
    suffix: usize,
    sum: u8,
}

impl Decoder {
    /// Starts an empty packet; consume each emitted packet immediately.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: State::Start,
            raw: Vec::new(),
            body: Vec::new(),
            chunk: 0,
            suffix: usize::MAX,
            sum: 0,
        }
    }

    /// Consumes one byte, retaining exact accepted input within fixed bounds.
    /// # Errors
    /// Returns sequence, checksum or size failures; further input is stopped.
    pub fn push(&mut self, byte: u8) -> Result<Option<Packet>, Error> {
        let result = self.consume(byte);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }

    fn consume(&mut self, byte: u8) -> Result<Option<Packet>, Error> {
        if matches!(self.state, State::Failed)
            || matches!(self.state, State::Done) && self.suffix == usize::MAX
        {
            return Err(Error::Stopped);
        }
        // Even an RS before every character plus CRLF stays below this ceiling.
        if self.raw.len() >= 8192 {
            return Err(Error::Payload);
        }
        self.raw.push(byte);
        if let Some(expected) = self.config.suffix.get(self.suffix) {
            if byte != *expected {
                return Err(Error::Sequence);
            }
            if self.config.checksum.include_suffix
                && (matches!(self.state, State::Text | State::HexFirst)
                    || matches!(self.state, State::Record) && self.config.checksum.include_stx)
            {
                self.sum = self.sum.wrapping_add(byte);
            }
            self.suffix = self.suffix.checked_add(1).ok_or(Error::Payload)?;
            if self.suffix == self.config.suffix.len() {
                self.suffix = usize::MAX;
            }
            return self.complete();
        }
        let control = matches!(byte, STX | RS | GS | ETX);
        match self.state {
            State::Start if byte == STX => {
                self.state = State::Record;
                if self.config.checksum.include_stx {
                    self.sum = byte;
                }
            }
            State::Record if byte == RS => {
                self.state = State::Text;
                self.sum = self.sum.wrapping_add(byte);
            }
            State::Text if byte == RS && self.chunk > 0 => {
                self.chunk = 0;
                self.sum = self.sum.wrapping_add(byte);
            }
            State::Text if byte == GS && self.chunk > 0 => {
                self.state = State::HexFirst;
                self.sum = self.sum.wrapping_add(byte);
            }
            State::Text if (32..=126).contains(&byte) => {
                if self.chunk >= self.config.chunk || self.body.len() >= self.config.maximum {
                    return Err(Error::Payload);
                }
                self.chunk = self.chunk.checked_add(1).ok_or(Error::Payload)?;
                self.body.push(byte);
                self.sum = self.sum.wrapping_add(byte);
            }
            State::HexFirst => self.state = State::HexLast(byte),
            State::HexLast(first) => {
                let received = [first, byte];
                let expected = hex(self.sum, self.config.checksum.hex.uppercase);
                let valid = if self.config.checksum.hex.accept_either_case {
                    received.eq_ignore_ascii_case(expected.as_bytes())
                } else {
                    received == expected.as_bytes()
                };
                if !valid {
                    return Err(Error::Checksum);
                }
                self.state = if self.config.mode == Mode::Literal {
                    State::Done
                } else {
                    State::End
                };
            }
            State::End if byte == ETX => self.state = State::Done,
            State::Start | State::Record | State::Text | State::End => return Err(Error::Sequence),
            State::Done | State::Failed => return Err(Error::Stopped),
        }
        if control && !self.config.suffix.is_empty() {
            self.suffix = 0;
        }
        self.complete()
    }

    fn complete(&mut self) -> Result<Option<Packet>, Error> {
        if matches!(self.state, State::Done)
            && (self.config.suffix.is_empty() || self.suffix == usize::MAX)
        {
            Ok(Some(Packet {
                raw: std::mem::take(&mut self.raw),
                body: std::mem::take(&mut self.body),
            }))
        } else {
            Ok(None)
        }
    }
}

/// Decodes exactly one retained acceptance boundary using the live decoder.
/// # Errors
/// Rejects incomplete packets, trailing bytes and every incremental framing defect.
pub fn decode(raw: &[u8], config: Config) -> Result<Packet, Error> {
    let mut decoder = Decoder::new(config);
    let mut packet = None;
    for byte in raw {
        if packet.is_some() {
            return Err(Error::Trailing);
        }
        packet = decoder.push(*byte)?;
    }
    packet.ok_or(Error::Incomplete)
}

/// STX control byte (BCI Appendix A chapter 3).
pub const STX: u8 = 0x02;
/// ETX control byte (BCI Appendix A chapter 3).
pub const ETX: u8 = 0x03;
/// EOT control byte (BCI Appendix A chapter 3).
pub const EOT: u8 = 0x04;
/// ENQ control byte (BCI Appendix A chapter 3).
pub const ENQ: u8 = 0x05;
/// ACK control byte (BCI Appendix A chapter 3).
pub const ACK: u8 = 0x06;
/// LF control byte (BCI Appendix A chapter 3).
pub const LF: u8 = 0x0a;
/// CR control byte (BCI Appendix A chapter 3).
pub const CR: u8 = 0x0d;
/// NAK control byte (BCI Appendix A chapter 3).
pub const NAK: u8 = 0x15;
/// GS control byte (BCI Appendix A chapter 3).
pub const GS: u8 = 0x1d;
/// RS control byte (BCI Appendix A chapter 3).
pub const RS: u8 = 0x1e;

impl crate::Decode for Decoder {
    type Event = Packet;
    type Error = Error;
    fn push_byte(&mut self, byte: u8) -> Result<Option<Packet>, Error> {
        self.push(byte)
    }
    fn finish_stream(&mut self) -> Result<Option<Packet>, Error> {
        if self.raw.is_empty() || matches!(self.state, State::Done) && self.suffix == usize::MAX {
            return Ok(None);
        }
        self.state = State::Failed;
        Err(Error::Incomplete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decode;
    use proptest::prelude::*;

    fn config(
        mode: Mode,
        suffix: &[u8],
        include_stx: bool,
        include_suffix: bool,
        uppercase: bool,
        chunk: usize,
    ) -> Config {
        Config::new(
            mode,
            chunk,
            1920,
            suffix,
            Checksum {
                include_stx,
                include_suffix,
                hex: HexOptions {
                    uppercase,
                    accept_either_case: false,
                },
            },
        )
        .unwrap_or_else(|error| panic!("synthetic configuration: {error}"))
    }

    #[test]
    fn vidas_bci_appendix_a_ch3_checksum_span_case_suffix_and_packet_boundaries() {
        for mode in [Mode::Literal, Mode::Alternate] {
            let c = config(mode, &[], false, false, false, 80);
            let mut expected = vec![STX, RS, b'A', GS, b'7', b'c'];
            if mode == Mode::Alternate {
                expected.push(ETX);
            }
            assert_eq!(c.encode(b"A").unwrap(), expected);
            assert_eq!(decode(&expected, c).unwrap().body, b"A");
        }
        for (stx, suffix, expected) in [
            (false, false, "7c"),
            (true, false, "7e"),
            (false, true, "96"),
            (true, true, "a5"),
        ] {
            let c = config(Mode::Literal, &[CR], stx, suffix, false, 80);
            let raw = c.encode(b"A").unwrap();
            assert!(raw.ends_with(expected.as_bytes()));
            assert_eq!(decode(&raw, c).unwrap().raw, raw);
        }
        let mut c = config(Mode::Literal, &[], false, false, true, 80);
        let lower = config(Mode::Literal, &[], false, false, false, 80)
            .encode(b"A")
            .unwrap();
        assert_eq!(decode(&lower, c.clone()).unwrap_err(), Error::Checksum);
        c.checksum.hex.accept_either_case = true;
        assert_eq!(decode(&lower, c).unwrap().body, b"A");
    }

    #[test]
    fn vidas_bci_appendix_a_ch3_malformed_packets_truncation_and_limits() {
        let c = config(Mode::Alternate, &[CR, LF], false, false, false, 1);
        let raw = c.encode(&vec![b'A'; 1920]).unwrap();
        assert_eq!(decode(&raw, c.clone()).unwrap().body.len(), 1920);
        assert_eq!(c.encode(&vec![b'A'; 1921]), Err(Error::Payload));
        assert_eq!(c.encode(b""), Err(Error::Payload));
        assert_eq!(c.encode(&[NAK]), Err(Error::Payload));
        let c = config(Mode::Alternate, &[], false, false, false, 80);
        let raw = c.encode(b"A").unwrap();
        for length in 0..raw.len() {
            assert!(decode(&raw[..length], c.clone()).is_err());
        }
        for (offset, byte) in [(0, EOT), (2, NAK), (5, b'0')] {
            let mut damaged = raw.clone();
            damaged[offset] = byte;
            let mut decoder = Decoder::new(c.clone());
            let mut failed = false;
            for byte in damaged {
                if decoder.push_byte(byte).is_err() {
                    failed = true;
                    break;
                }
            }
            assert!(failed);
            assert_eq!(decoder.push_byte(b'A').unwrap_err(), Error::Stopped);
        }
        let mut trailing = raw;
        trailing.push(EOT);
        assert_eq!(decode(&trailing, c.clone()).unwrap_err(), Error::Trailing);
        for (chunk, maximum, suffix) in [
            (0, 1920, &[][..]),
            (81, 1920, &[][..]),
            (80, 0, &[][..]),
            (80, 1921, &[][..]),
            (80, 1920, &[EOT][..]),
        ] {
            assert!(Config::new(Mode::Literal, chunk, maximum, suffix, c.checksum).is_err());
        }
    }

    proptest! {
        #[test]
        fn vidas_bci_appendix_a_ch3_property_roundtrip_both_modes_and_all_checksum_options(body in "[ -~]{1,1920}", alternate in any::<bool>(), suffix in 0usize..4, stx in any::<bool>(), include_suffix in any::<bool>(), uppercase in any::<bool>(), chunk in 1usize..81, split in 0usize..9000) {
            let suffixes = [&[][..], &[CR][..], &[LF][..], &[CR,LF][..]];
            let c = config(if alternate {Mode::Alternate} else {Mode::Literal}, suffixes[suffix], stx, include_suffix, uppercase, chunk);
            let raw = c.encode(body.as_bytes()).unwrap();
            let mut decoder = Decoder::new(c);
            let split = split.min(raw.len());
            let mut output = Vec::new();
            for fragment in [&raw[..split], &raw[split..]] {
                for byte in fragment { if let Some(packet) = decoder.push_byte(*byte).unwrap() { output.push(packet); } }
            }
            prop_assert_eq!(output.len(), 1);
            prop_assert_eq!(&output[0].body, body.as_bytes());
            prop_assert_eq!(&output[0].raw, &raw);
            prop_assert!(decoder.finish_stream().unwrap().is_none());
        }
    }
}
