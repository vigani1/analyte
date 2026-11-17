//! Encodes bounded MLLP transport writes for HL7 and LAW dialogs.
use analyte_framing::hl7 as framing;
use std::time::Duration;

/// Exact transport bytes and an optional absolute dialog issuance deadline.
#[derive(Debug)]
pub struct Write {
    /// Framed bytes, preserved in wire order.
    pub bytes: Vec<u8>,
    /// Absolute connection-clock deadline, or the host transport budget.
    pub until: Option<Duration>,
}
impl Write {
    pub(crate) fn encode(
        raw: &[u8],
        limit: framing::Limit,
        until: Option<Duration>,
    ) -> Result<Self, framing::Error> {
        Ok(Self {
            bytes: framing::encode(raw, limit)?,
            until,
        })
    }
}
