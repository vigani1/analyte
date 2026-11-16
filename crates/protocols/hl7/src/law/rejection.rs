//! Builds negative LAW replies when transaction validation fails.
use super::{Error, content, responses::Wire};
use crate::{AckCode, AckError, Message, Options, Response};

/// Builds a correlated negative application ACK from a recoverable LAW header.
/// Original input remains untouched. No response is made to ACK/ORL/RSP or MSA traffic.
/// This recovery path uses the manager's supported version, not an invalid peer version.
/// # Errors
/// Reports unsafe correlation/routing fields, output limits or an invalid response identity.
pub fn reject_trigger(
    bytes: &[u8],
    response: Response<'_>,
    code: AckCode,
    error: AckError<'_>,
    options: Options,
) -> Result<Option<Message>, Error> {
    if !matches!(code, AckCode::Ae | AckCode::Ar) {
        return Err(content("MSA", 1));
    }
    if bytes.len() > options.limits.message_bytes || !bytes.starts_with(b"MSH|^~\\&|") {
        return Ok(None);
    }
    let header = bytes
        .split(|b| *b == b'\r')
        .next()
        .ok_or(content("MSH", 0))?;
    let fields = header
        .split(|b| *b == b'|')
        .take(options.limits.fields.saturating_add(1))
        .collect::<Vec<_>>();
    if fields.len() > options.limits.fields {
        return Ok(None);
    }
    let get = |number: usize| {
        fields
            .get(number.saturating_sub(1))
            .copied()
            .unwrap_or_default()
    };
    let kind = get(9).split(|b| *b == b'^').next().unwrap_or_default();
    if matches!(kind, b"ACK" | b"ORL" | b"RSP")
        || bytes.split(|b| *b == b'\r').any(|s| s.starts_with(b"MSA|"))
    {
        return Ok(None);
    }
    if super::control_id(get(10)).is_err() {
        return Ok(None);
    }
    for field in [3, 4, 5, 6] {
        if get(field).len() > 227
            || get(field)
                .iter()
                .any(|b| b.is_ascii_control() || b"|~\\".contains(b))
        {
            return Ok(None);
        }
    }
    super::control_id(response.control_id.raw())?;
    if response.control_id.raw() == get(10) || !super::profile::timestamp(response.timestamp, true)
    {
        return Err(content("MSH", 10));
    }
    let event = get(9).split(|b| *b == b'^').nth(1).unwrap_or_default();
    if event.len() > 3 || !event.iter().all(u8::is_ascii_alphanumeric) {
        return Ok(None);
    }
    // Construct only a recoverable control view. This is never persisted as source evidence.
    // Use our declared version/charset so invalid peer control content cannot prevent rejection.
    let mut wire = Wire::new(options.limits.message_bytes, b'|');
    wire.append(b"MSH")?;
    for field in [
        b"^~\\&".as_slice(),
        get(3),
        get(4),
        get(5),
        get(6),
        response.timestamp,
        b"",
        get(9),
        get(10),
        b"P",
        b"2.5.1",
        b"",
        b"",
        b"NE",
        b"AL",
        b"",
        b"UNICODE UTF-8",
    ] {
        wire.field(field)?;
    }
    wire.append(b"\r")?;
    let header = super::parse_message(&wire.bytes, options)?;
    let msh = crate::Msh::from_law(header.segments().next().ok_or(content("MSH", 0))?, true);
    let Some(ack) = crate::ack::acknowledgment_with_msh(
        msh,
        response,
        code,
        crate::Responsibility::Stored,
        Some(error),
        options,
        crate::ack::AckProfile::Law,
    )?
    else {
        return Ok(None);
    };
    let mut wire = Wire::new(options.limits.message_bytes, b'|');
    wire.append(ack.segments().next().ok_or(content("MSH", 0))?.raw())?;
    wire.append(b"\r")?;
    super::responses::append_ack_segments(&mut wire, &ack)?;
    Ok(Some(super::parse_message(&wire.bytes, options)?))
}
