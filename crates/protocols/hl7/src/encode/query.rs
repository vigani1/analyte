//! Builds legacy HL7 QCK/Q02 and DSR/Q03 display query replies (2.3.1 section 2.18 and 2.5.1 section 5.10).
use crate::query::{Profile, Request, invalid, raw};
use crate::{AckCode, Message, Msh, Version};

/// Trusted cache result; lookup failure has no successful empty representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAnswer {
    /// Trusted lookup found matching data.
    Found,
    /// Trusted lookup found no matching data.
    Empty,
    /// Lookup could not be trusted, including an unavailable lab app.
    Unavailable,
}

/// Builds QCK/Q02 after the host durably stores the QRY, with explicit cache status.
///
/// # Errors
/// Rejects unsafe identity/time fields, enhanced acknowledgment negotiation and limits.
pub fn acknowledgment(
    request: Request<'_>,
    response: crate::Response<'_>,
    answer: CacheAnswer,
    options: crate::Options,
) -> Result<Message, crate::Error> {
    let (code, status, responsibility) = match answer {
        CacheAnswer::Found => (
            AckCode::Aa,
            b"OK".as_slice(),
            crate::Responsibility::StoredAndInterpreted,
        ),
        CacheAnswer::Empty => (
            AckCode::Aa,
            b"NF".as_slice(),
            crate::Responsibility::StoredAndInterpreted,
        ),
        CacheAnswer::Unavailable => (AckCode::Ae, b"AE".as_slice(), crate::Responsibility::Stored),
    };
    let msh = Msh::new(request.message)?;
    if msh.mode() != crate::AckMode::Original {
        return Err(crate::Error::AckMode);
    }
    let ack = crate::acknowledgment(
        request.message,
        response,
        code,
        responsibility,
        None,
        options,
    )?
    .ok_or(crate::Error::AckMode)?;
    let d = request.message.delimiters();
    let mut output = Vec::new();
    encode_response_segments(
        &mut output,
        &ack,
        msh.version(),
        (b"QCK", b"Q02", b"QCK_Q02"),
    );
    if request.profile == Profile::RecordDisplay {
        output.extend_from_slice(b"ERR");
        output.push(d.field);
        output.extend_from_slice(if answer == CacheAnswer::Unavailable {
            b"207"
        } else {
            b"0"
        });
        output.push(b'\r');
    }
    output.extend_from_slice(b"QAK");
    output.push(d.field);
    if request.profile == Profile::RecordDisplay {
        output.extend_from_slice(b"SR");
    }
    output.push(d.field);
    output.extend_from_slice(status);
    output.push(b'\r');
    Message::parse(&output, options)
}

/// Builds a complete DSR/Q03 for a trusted lookup with display lines.
///
/// The caller supplies presentation text under its configured interface. This
/// function assigns no laboratory meaning to a line position. NF and AE replies
/// terminate at QCK; they cannot accidentally produce an empty positive DSR.
///
/// # Errors
/// Rejects empty data, unsupported quantity units, limits and delimiter injection.
pub fn display(
    request: Request<'_>,
    response: crate::Response<'_>,
    qck_id: crate::ControlId<'_>,
    lines: &[&[u8]],
    options: crate::Options,
) -> Result<Message, crate::Error> {
    if request.profile != Profile::Display || response.control_id == qck_id {
        return Err(invalid("MSH", 10));
    }
    if lines.is_empty() || lines.len() > options.limits.segments {
        return Err(invalid("DSP", 0));
    }
    let d = request.message.delimiters();
    let mut quantity = raw(request.definition, 7).split(|b| *b == d.component);
    let amount = quantity.next().unwrap_or_default();
    let maximum = amount
        .iter()
        .try_fold(0_usize, |value, digit| {
            value
                .checked_mul(10)?
                .checked_add(usize::from(digit.checked_sub(b'0')?))
        })
        .ok_or(invalid("QRD", 7))?;
    // Page, record and local units require an interface-specific pagination contract.
    if !matches!(quantity.next(), None | Some(b"" | b"LI")) || lines.len() > maximum {
        return Err(invalid("QRD", 7));
    }
    let qck = acknowledgment(request, response, CacheAnswer::Found, options)?;
    let mut output = Vec::new();
    encode_response_segments(
        &mut output,
        &qck,
        Msh::new(request.message)?.version(),
        (b"DSR", b"Q03", b"DSR_Q03"),
    );
    output.extend_from_slice(request.definition.raw());
    output.push(b'\r');
    if let Some(filter) = request.filter {
        output.extend_from_slice(filter.raw());
        output.push(b'\r');
    }
    encode_lines(&mut output, request, lines, options)?;
    Message::parse(&output, options)
}

/// Builds one bounded positional display page with a continuation pointer.
/// The caller provides a distinct control ID for each page.
///
/// # Errors
/// Refuses missing required rows, invalid profile and unrepresentable data.
pub fn record_display(
    request: Request<'_>,
    response: crate::Response<'_>,
    lines: &[Vec<u8>],
    continuation: Option<usize>,
    options: crate::Options,
) -> Result<Message, crate::Error> {
    if request.profile != Profile::RecordDisplay
        || !(29..=9999).contains(&lines.len())
        || lines.len() > options.limits.segments
        || lines.get(20).is_none_or(Vec::is_empty)
        || lines.iter().skip(28).any(Vec::is_empty)
    {
        return Err(invalid("DSP", 0));
    }
    let d = request.message.delimiters();
    let qck = acknowledgment(request, response, CacheAnswer::Found, options)?;
    let mut output = Vec::new();
    encode_response_segments(
        &mut output,
        &qck,
        Version::V231,
        (b"DSR", b"Q03", b"DSR_Q03"),
    );
    // YHLO LIS V1.1 section 2.4.2 places the barcode in DSP-21.
    for (index, field) in request
        .definition
        .raw()
        .split(|b| *b == d.field)
        .enumerate()
    {
        if index != 0 {
            output.push(d.field);
        }
        if index != 8 {
            output.extend_from_slice(field);
        }
    }
    output.push(b'\r');
    if let Some(filter) = request.filter {
        output.extend_from_slice(filter.raw());
        output.push(b'\r');
    }
    encode_lines(&mut output, request, lines, options)?;
    output.extend_from_slice(b"DSC");
    output.push(d.field);
    if let Some(pointer) = continuation {
        output.extend_from_slice(pointer.to_string().as_bytes());
    }
    output.push(b'\r');
    Message::parse(&output, options)
}

fn encode_lines(
    output: &mut Vec<u8>,
    request: Request<'_>,
    lines: &[impl AsRef<[u8]>],
    options: crate::Options,
) -> Result<(), crate::Error> {
    let d = request.message.delimiters();
    for (index, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        let assay = request.profile == Profile::RecordDisplay && index >= 28;
        let maximum = if assay { 297 } else { 300 };
        if line.len() > maximum
            || line
                .iter()
                .any(|b| !b.is_ascii() || b.is_ascii_control() || d.bytes().contains(b))
        {
            return Err(invalid("DSP", 3));
        }
        let number = index.checked_add(1).ok_or(invalid("DSP", 1))?;
        output.extend_from_slice(b"DSP");
        output.push(d.field);
        output.extend_from_slice(number.to_string().as_bytes());
        output.push(d.field);
        output.push(d.field);
        output.extend_from_slice(line);
        if assay {
            output.extend_from_slice(&[d.component; 3]);
        }
        output.push(b'\r');
        if output.len() > options.limits.message_bytes {
            return Err(invalid("DSP", 0));
        }
    }
    Ok(())
}

// Both query replies retain the acknowledgment fields and replace only MSH-9.
fn encode_response_segments(
    output: &mut Vec<u8>,
    message: &Message,
    version: Version,
    kind: (&[u8], &[u8], &[u8]),
) {
    let (message_type, event, structure) = kind;
    let d = message.delimiters();
    for segment in message.segments() {
        if segment.id() == b"MSH" {
            output.extend_from_slice(b"MSH");
            for field in 2..=18 {
                output.push(d.field);
                if field == 9 {
                    output.extend_from_slice(message_type);
                    output.push(d.component);
                    output.extend_from_slice(event);
                    if version.legacy_query_structure() {
                        output.push(d.component);
                        output.extend_from_slice(structure);
                    }
                } else {
                    output.extend_from_slice(raw(segment, field));
                }
            }
        } else {
            output.extend_from_slice(segment.raw());
        }
        output.push(b'\r');
    }
}
