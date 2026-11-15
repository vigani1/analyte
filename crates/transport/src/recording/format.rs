//! ADR 0009 bounded JSONL codec shared by capture and replay.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// Independent resource limits, applied before growing input buffers.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Maximum complete file bytes.
    pub file_bytes: usize,
    /// Maximum JSON line bytes including LF.
    pub line_bytes: usize,
    /// Maximum events.
    pub events: usize,
    /// Maximum decoded bytes per event.
    pub event_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            file_bytes: 16 * 1024 * 1024,
            line_bytes: 16384,
            events: 100000,
            event_bytes: crate::CHUNK_LIMIT,
        }
    }
}
/// Capture provenance; never inferred from replay success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Physical machine capture.
    Machine,
    /// Vendor-provided conversation.
    VendorTrace,
    /// Simulator capture.
    Simulator,
    /// Invented test data.
    Synthetic,
}
/// Versioned recording metadata. Unknown machine information stays absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    /// ADR format version, currently one.
    pub version: u8,
    /// Transport, framing, dialog and syntax names, in that order.
    pub stack: [String; 4],
    /// Machine name when known.
    pub machine: Option<String>,
    /// Machine software when known.
    pub software: Option<String>,
    /// Origin of the bytes.
    pub evidence: Evidence,
    /// Explicit privacy statement.
    pub de_identification: String,
    /// Source document or capture method in words.
    pub source: String,
    /// Named fixed host byte locations allowed to vary during replay.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<Exception>,
}
impl Header {
    pub(super) fn live() -> Self {
        Self {
            version: 1,
            stack: std::array::from_fn(|_| "unknown".into()),
            machine: None,
            software: None,
            evidence: Evidence::Machine,
            de_identification: "NOT de-identified; protected store only".into(),
            source: "Host transport wire tap".into(),
            exceptions: Vec::new(),
        }
    }
}
/// Wire direction, independent of socket read boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Machine transmission.
    MachineToHost,
    /// Host transmission.
    HostToMachine,
}
/// Physical link transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// Link opened.
    Connect,
    /// Link closed.
    Disconnect,
    /// Serial break.
    Break,
}
/// Exact wire bytes or link event. Times are relative milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Sender.
    pub direction: Direction,
    /// Time since first event.
    pub time_ms: u64,
    /// Exact decoded bytes; link events may carry settings or reasons.
    pub bytes: Vec<u8>,
    /// Optional link transition.
    pub link: Option<Link>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEvent {
    direction: Direction,
    time_ms: u64,
    bytes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link: Option<Link>,
}
/// Line-specific failure; never includes clinical input.
#[derive(Debug, thiserror::Error)]
#[error("recording line {line}: {reason}")]
pub struct FormatError {
    /// One-based failing line.
    pub line: usize,
    /// Nonclinical diagnostic.
    pub reason: String,
}
fn error(line: usize, reason: &str) -> FormatError {
    FormatError {
        line,
        reason: reason.into(),
    }
}
/// Reads one recording with bounded input growth and strict line handling.
/// # Errors
/// Rejects malformed, unsupported, truncated or over-limit input at its line.
pub fn read(mut input: impl Read, limits: Limits) -> Result<(Header, Vec<Event>), FormatError> {
    let mut line = Vec::new();
    let mut total = 0usize;
    let mut number = 1usize;
    let mut header = None;
    let mut events = Vec::new();
    loop {
        let mut byte = [0];
        let n = input
            .read(&mut byte)
            .map_err(|_| error(number, "read failed"))?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(1)
            .ok_or_else(|| error(number, "file limit"))?;
        if total > limits.file_bytes || line.len() >= limits.line_bytes {
            return Err(error(number, "file or line limit"));
        }
        if number > 1 && events.len() >= limits.events {
            return Err(error(number, "event count limit"));
        }
        line.push(byte[0]);
        if byte[0] != b'\n' {
            continue;
        }
        if number == 1 {
            let value: Header =
                serde_json::from_slice(&line).map_err(|_| error(number, "invalid header"))?;
            if value.version != 1
                || value.source.is_empty()
                || value.de_identification.is_empty()
                || value.stack.iter().any(String::is_empty)
            {
                return Err(error(number, "unsupported or incomplete header"));
            }
            header = Some(value);
        } else {
            let value: WireEvent =
                serde_json::from_slice(&line).map_err(|_| error(number, "invalid event"))?;
            // Compute exact decoded size before allocating a decoded byte buffer.
            let padding = value
                .bytes
                .as_bytes()
                .iter()
                .rev()
                .take_while(|b| **b == b'=')
                .count();
            let decoded = value
                .bytes
                .len()
                .checked_div(4)
                .and_then(|n| n.checked_mul(3))
                .and_then(|n| n.checked_sub(padding))
                .ok_or_else(|| error(number, "invalid base64 size"))?;
            if decoded > limits.event_bytes {
                return Err(error(number, "event byte limit"));
            }
            let bytes = STANDARD
                .decode(&value.bytes)
                .map_err(|_| error(number, "invalid base64"))?;
            events.push(Event {
                direction: value.direction,
                time_ms: value.time_ms,
                bytes,
                link: value.link,
            });
        }
        line.clear();
        number = number
            .checked_add(1)
            .ok_or_else(|| error(number, "line count overflow"))?;
    }
    if !line.is_empty() {
        return Err(error(number, "missing final LF"));
    }
    Ok((header.ok_or_else(|| error(1, "missing header"))?, events))
}
fn write_line(output: &mut impl Write, value: &impl Serialize, limits: Limits) -> io::Result<()> {
    let line = serde_json::to_vec(value)?;
    if line.len() >= limits.line_bytes {
        return Err(io::Error::other("recording line limit"));
    }
    output.write_all(&line)?;
    output.write_all(b"\n")
}
/// Writes one header; caller owns file size and event count across appends.
/// # Errors
/// Returns serialization, line bound or output failures.
pub fn write_header(output: &mut impl Write, header: &Header, limits: Limits) -> io::Result<()> {
    write_line(output, header, limits)
}
/// Writes one bounded event using the same schema as the reader.
/// # Errors
/// Returns byte/line bound or output failures.
pub fn write_event(output: &mut impl Write, event: &Event, limits: Limits) -> io::Result<()> {
    if event.bytes.len() > limits.event_bytes {
        return Err(io::Error::other("recording event byte limit"));
    }
    write_line(
        output,
        &WireEvent {
            direction: event.direction,
            time_ms: event.time_ms,
            bytes: STANDARD.encode(&event.bytes),
            link: event.link,
        },
        limits,
    )
}

/// A named fixed byte location allowed to differ in one recorded host event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    /// Reason, such as a generated MSH-10 control ID.
    pub name: String,
    /// Zero-based event index.
    pub event: usize,
    /// Fixed byte range, never a pattern.
    pub bytes: std::ops::Range<usize>,
}
