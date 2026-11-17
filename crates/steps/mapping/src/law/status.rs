//! Maps status-only LAW groups as execution evidence, never clinical observations.
use super::Error;
use crate::{Output, Position, text};
use analyte_hl7::law::{ExecutionStatus, Results};
use analyte_model::{AwosExecution, ProcessingMode, SourceText, UtcOffset};

/// An availability estimate preserves explicit null separately from absence.
#[derive(Debug)]
pub enum AvailabilityEstimate {
    /// No new estimate evidence; does not erase an earlier estimate.
    Absent,
    /// Explicit source null, retained as an event.
    Null,
    /// Supplied date or date/time, with an offset derived solely from the header.
    Supplied {
        /// Exact ORC-27 spelling, never replaced with a normalized timestamp.
        original: SourceText,
        /// Exact MSH-7 used for the derivation, including missing offset evidence.
        header_time: SourceText,
        /// Header offset only; None means unknown.
        offset_from_header: Option<UtcOffset>,
    },
}

/// A non-result interpretation of one validated empty AWOS group.
#[derive(Debug)]
pub struct StatusNotice {
    /// Source OBR position for located correlation failures.
    pub position: Position,
    /// Explicit source processing mode; production admission remains a host responsibility.
    pub processing: ProcessingMode,
    /// Independent analyzer execution status.
    pub execution: AwosExecution,
    /// Complete original OBR, including ignored authority evidence.
    pub order: SourceText,
    /// Original specimen context.
    pub specimen: SourceText,
    /// Original container context.
    pub container: SourceText,
    /// Original ORC, including control, status and source times.
    pub control: SourceText,
    /// Availability estimate, independent of completion and deadlines.
    pub estimate: AvailabilityEstimate,
}

pub(super) fn map(
    results: &Results<'_>,
    source: &crate::inbound::source::Source<'_>,
    output: &mut Output,
) -> Result<(), Error> {
    let message = results.message();
    let header = message.segments().next().ok_or(Error::MissingHeader)?;
    let header_time = decode(
        header.field(7).map_or(b"", |f| f.raw()),
        Position {
            record: 1,
            field: 7,
        },
    )?;
    let header_parsed = crate::inbound::time::parse(
        header_time.as_str(),
        Position {
            record: 1,
            field: 7,
        },
    )?;
    let processing = match header
        .field(11)
        .and_then(|f| f.repetitions().next())
        .and_then(|r| r.components().next())
        .map(analyte_hl7::Component::raw)
    {
        Some(b"P") => ProcessingMode::Production,
        Some(b"T") => ProcessingMode::Training,
        Some(b"D") => ProcessingMode::Debug,
        Some(_) | None => ProcessingMode::Unknown,
    };
    for notice in results.status_notices() {
        let position = Position {
            record: notice
                .order_index()
                .checked_add(1)
                .ok_or(Error::NoticePosition)?,
            field: 2,
        };
        crate::reserve_evidence(
            output,
            source,
            position
                .record
                .checked_sub(1)
                .ok_or(Error::NoticePosition)?,
        )?;
        let original = notice
            .control()
            .field(27)
            .map_or(b"".as_slice(), |f| f.raw());
        let estimate = match original {
            b"" => AvailabilityEstimate::Absent,
            b"\"\"" => AvailabilityEstimate::Null,
            value => AvailabilityEstimate::Supplied {
                original: decode(value, position)?,
                header_time: header_time.clone(),
                offset_from_header: header_parsed.offset,
            },
        };
        output.status_notices.push(StatusNotice {
            position,
            processing,
            execution: match notice.status() {
                ExecutionStatus::Scheduled => AwosExecution::Scheduled,
                ExecutionStatus::InProgress => AwosExecution::InProgress,
                ExecutionStatus::AnalyzerCancelled => AwosExecution::AnalyzerCancelled,
            },
            order: decode(notice.order().raw(), position)?,
            specimen: decode(notice.specimen().raw(), position)?,
            container: decode(notice.container().raw(), position)?,
            control: decode(notice.control().raw(), position)?,
            estimate,
        });
    }
    Ok(())
}

fn decode(raw: &[u8], position: Position) -> Result<SourceText, Error> {
    Ok(text(
        std::str::from_utf8(raw).map_err(|_| crate::Error::Encoding { position })?,
        position,
    )?)
}
