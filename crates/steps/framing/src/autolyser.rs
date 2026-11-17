//! Frames Autolyser checksum envelopes (Rev18 chapter 23).
use thiserror::Error;
/// Specific record defect; display never contains patient data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Input arrived after this response was emitted.
    #[error("response decoder stopped")]
    Stopped,
    /// Wrong field or record width.
    #[error("incorrect width")]
    Length,
    /// Invalid field repertoire or discriminator.
    #[error("invalid field: {0}")]
    Field(&'static str),
    /// Decimal checksum differs from the payload sum.
    #[error("checksum mismatch")]
    Checksum,
    /// Count exceeds the wire representation.
    #[error("count outside wire bound")]
    Count,
}

/// Sum modulo 256, excluding framing and the checksum digits (Rev18 p.179).
#[must_use]
pub fn checksum(payload: &[u8]) -> u8 {
    payload.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

/// Appends the decimal checksum and EOT to an encoded record payload.
#[must_use]
pub fn finish(mut payload: Vec<u8>) -> Vec<u8> {
    payload.extend_from_slice(format!("{:03}", checksum(&payload)).as_bytes());
    payload.push(EOT);
    payload
}

/// Returns the payload after checking the bounded checksum envelope.
/// # Errors
/// Rejects invalid lengths, decimal digits and checksums.
pub fn verified(raw: &[u8]) -> Result<&[u8], Error> {
    if raw.len() > 11038 || raw.last() != Some(&EOT) {
        return Err(Error::Length);
    }
    let end = raw.len().checked_sub(4).ok_or(Error::Length)?;
    let payload = raw.get(..end).ok_or(Error::Length)?;
    let digits = raw
        .get(end..raw.len().saturating_sub(1))
        .ok_or(Error::Length)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return Err(Error::Field("decimal"));
    }
    let value = std::str::from_utf8(digits)
        .map_err(|_| Error::Field("decimal"))?
        .parse::<u16>()
        .map_err(|_| Error::Count)?;
    if value != u16::from(checksum(payload)) {
        return Err(Error::Checksum);
    }
    Ok(payload)
}

/// Start of an Autolyser exchange (Rev18 p.175).
pub const STX: u8 = 0x02;
/// Positive initialization acknowledgment (Rev18 p.175).
pub const ACK: u8 = 0x06;
/// End of a poll or report (Rev18 pp.177-179).
pub const EOT: u8 = 0x04;
/// Empty report queue (Rev18 p.177).
pub const NAK: u8 = 0x15;

/// Expected response layout, selected by the dialog's current operation.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// Two binary insertion reply bytes, including control-valued positions.
    Insertion,
    /// A report envelope or standalone empty-queue indication.
    Report,
}

/// Retains exact response evidence, including incomplete responses.
#[derive(Debug)]
pub struct Frame {
    raw: Vec<u8>,
    complete: bool,
}
impl Frame {
    /// Returns the original bytes for durable storage.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Reports whether a protocol boundary was received before the byte ceiling.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.complete
    }
    /// Checks the report checksum after the host has stored the raw evidence.
    /// # Errors
    /// Rejects incomplete or malformed checksum envelopes.
    pub fn body(&self) -> Result<&[u8], Error> {
        if !self.complete {
            return Err(Error::Length);
        }
        verified(&self.raw)
    }
}

/// Collects one bounded response without depending on transport read boundaries.
#[derive(Debug)]
pub struct Decoder {
    mode: Mode,
    maximum: usize,
    raw: Vec<u8>,
    stopped: bool,
}
impl Decoder {
    /// Selects an explicit response layout and byte ceiling.
    /// # Errors
    /// Rejects ceilings outside the protocol's supported range.
    pub fn new(mode: Mode, maximum: usize) -> Result<Self, Error> {
        if !(2..=11038).contains(&maximum) {
            return Err(Error::Length);
        }
        Ok(Self {
            mode,
            maximum,
            raw: Vec::new(),
            stopped: false,
        })
    }
    /// Accepts one byte; a ceiling hit emits incomplete evidence and stops.
    /// # Errors
    /// Rejects input after a response has been emitted.
    pub fn push(&mut self, byte: u8) -> Result<Option<Frame>, Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        self.raw.push(byte);
        let complete = match self.mode {
            Mode::Insertion => self.raw.len() == 2,
            Mode::Report => byte == EOT || (self.raw.len() == 1 && byte == NAK),
        };
        if complete || self.raw.len() >= self.maximum {
            self.stopped = true;
            return Ok(Some(Frame {
                raw: std::mem::take(&mut self.raw),
                complete,
            }));
        }
        Ok(None)
    }
    /// Stops decoding and retains every partial byte for durable storage.
    #[must_use]
    pub fn finish(&mut self) -> Frame {
        self.stopped = true;
        Frame {
            raw: std::mem::take(&mut self.raw),
            complete: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn autolyser_rev18_p175_binary_insertion_reply_has_two_bytes() {
        for detail in [STX, EOT, ACK, NAK] {
            let mut decoder = Decoder::new(Mode::Insertion, 2).unwrap();
            assert!(decoder.push(b'Y').unwrap().is_none());
            let frame = decoder.push(detail).unwrap().unwrap();
            assert!(frame.complete());
            assert_eq!(frame.raw(), [b'Y', detail]);
            assert!(decoder.push(detail).is_err());
        }
    }
    #[test]
    fn autolyser_rev18_p177_179_boundaries_checksum_and_partial_evidence() {
        let raw = finish(b"synthetic".to_vec());
        let mut decoder = Decoder::new(Mode::Report, 100).unwrap();
        for byte in raw.iter().take(raw.len() - 1) {
            assert!(decoder.push(*byte).unwrap().is_none());
        }
        let frame = decoder.push(EOT).unwrap().unwrap();
        assert_eq!(frame.raw(), raw);
        assert_eq!(frame.body().unwrap(), b"synthetic");
        let mut decoder = Decoder::new(Mode::Report, 2).unwrap();
        assert!(decoder.push(b'x').unwrap().is_none());
        let frame = decoder.push(b'y').unwrap().unwrap();
        assert!(!frame.complete());
        assert_eq!(frame.raw(), b"xy");
        assert_eq!(frame.body(), Err(Error::Length));
        let mut decoder = Decoder::new(Mode::Report, 10).unwrap();
        decoder.push(b'x').unwrap();
        assert_eq!(decoder.finish().raw(), b"x");
        let mut decoder = Decoder::new(Mode::Report, 10).unwrap();
        assert_eq!(decoder.push(NAK).unwrap().unwrap().raw(), [NAK]);
    }

    #[test]
    fn autolyser_rev18_p179_decimal_checksum_malformed_envelopes_and_size_limits() {
        assert_eq!(finish(b"A".to_vec()), [b'A', b'0', b'6', b'5', EOT]);
        for raw in [
            &b"A066\x04"[..],
            &b"A0x5\x04"[..],
            &b"A065"[..],
            &b"065"[..],
        ] {
            assert!(verified(raw).is_err());
        }
        let raw = finish(vec![b'A'; 11034]);
        assert_eq!(raw.len(), 11038);
        assert_eq!(verified(&raw).unwrap().len(), 11034);
        assert_eq!(verified(&finish(vec![b'A'; 11035])), Err(Error::Length));
        for maximum in [0, 1, 11039] {
            assert!(Decoder::new(Mode::Report, maximum).is_err());
        }
    }

    proptest! {
        #[test]
        fn autolyser_rev18_p177_179_property_report_roundtrip_and_fragment_boundaries(body in prop::collection::vec(32u8..127,0..11035), split in 0usize..11039) {
            let raw = finish(body.clone());
            let mut decoder = Decoder::new(Mode::Report,11038).unwrap();
            let split = split.min(raw.len());
            let mut output = Vec::new();
            for fragment in [&raw[..split], &raw[split..]] {
                for byte in fragment { if let Some(frame) = decoder.push(*byte).unwrap() { output.push(frame); } }
            }
            prop_assert_eq!(output.len(),1);
            prop_assert!(output[0].complete());
            prop_assert_eq!(output[0].raw(), raw.as_slice());
            prop_assert_eq!(output[0].body().unwrap(), body);
            prop_assert!(decoder.finish().raw().is_empty());
        }
        #[test]
        fn autolyser_rev18_p175_property_binary_insertion_and_stopped_decoder(prefix in any::<u8>(), detail in any::<u8>()) {
            let mut decoder = Decoder::new(Mode::Insertion,2).unwrap();
            prop_assert!(decoder.push(prefix).unwrap().is_none());
            let frame = decoder.push(detail).unwrap().unwrap();
            prop_assert_eq!(frame.raw(), &[prefix,detail]);
            prop_assert!(frame.complete());
            prop_assert_eq!(decoder.push(prefix).unwrap_err(), Error::Stopped);
        }
        #[test]
        fn autolyser_rev18_p177_property_partial_evidence_is_exact_and_bounded(body in prop::collection::vec(32u8..127,0..1024), maximum in 2usize..1024) {
            let mut decoder = Decoder::new(Mode::Report,maximum).unwrap();
            let kept = body.len().min(maximum);
            let mut output = None;
            for byte in body.iter().take(kept) { output = decoder.push(*byte).unwrap(); }
            let frame = output.unwrap_or_else(|| decoder.finish());
            prop_assert!(!frame.complete());
            prop_assert_eq!(frame.raw(), &body[..kept]);
        }
    }
}
