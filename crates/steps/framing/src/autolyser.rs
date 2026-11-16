//! Frames Autolyser checksum envelopes (Rev18 chapter 23).
use thiserror::Error;
/// Specific record defect; display never contains patient data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
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
    payload.push(4);
    payload
}

/// Returns the payload after checking the bounded checksum envelope.
/// # Errors
/// Rejects invalid lengths, decimal digits and checksums.
pub fn verified(raw: &[u8]) -> Result<&[u8], Error> {
    if raw.len() > 11038 || raw.last() != Some(&4) {
        return Err(Error::Length);
    }
    let end = raw.len().checked_sub(4).ok_or(Error::Length)?;
    let payload = raw.get(..end).ok_or(Error::Length)?;
    let digits = raw
        .get(end..raw.len().saturating_sub(1))
        .ok_or(Error::Length)?;
    if number(digits)? != usize::from(checksum(payload)) {
        return Err(Error::Checksum);
    }
    Ok(payload)
}

fn number(bytes: &[u8]) -> Result<usize, Error> {
    bytes.iter().try_fold(0usize, |n, b| {
        if !b.is_ascii_digit() {
            return Err(Error::Field("decimal"));
        }
        n.checked_mul(10)
            .and_then(|v| v.checked_add(usize::from(b.saturating_sub(b'0'))))
            .ok_or(Error::Count)
    })
}
