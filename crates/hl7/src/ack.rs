use crate::{AckCode, AckMode, Error, Message, Msh, Options, Version};

/// Caller-reported processing state, with no implicit positive default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Responsibility {
    /// No durable responsibility accepted.
    NotStored,
    /// Bytes durably stored, interpretation unsuccessful or pending.
    Stored,
    /// Stored and successfully interpreted under the agreed profile.
    StoredAndInterpreted,
}
/// Caller-supplied response identity; validated before use on the wire.
#[derive(Debug, Clone, Copy)]
pub struct Response<'a> {
    /// New MSH-10, never the triggering control ID.
    pub control_id: crate::ControlId<'a>,
    /// Response creation time, supplied by host.
    pub timestamp: &'a [u8],
}
/// Structured error rendering inputs; values are bounded and delimiter-safe.
#[derive(Debug, Clone, Copy)]
pub struct AckError<'a> {
    /// Segment with the error, e.g. MSH; empty only for [`Self::unlocated`].
    pub segment: &'a [u8],
    /// Segment occurrence, starting at one; zero only for an unlocated LAW error.
    pub occurrence: u16,
    /// Field position, starting at one; zero only for an unlocated LAW error.
    pub field: u16,
    /// HL7 table 0357 code.
    pub code: &'a [u8],
    /// Optional local application error identifier.
    pub application_code: &'a [u8],
}

impl<'a> AckError<'a> {
    /// Creates an error without an invented location (LAW TF-2b C.1, optional ERR-2).
    /// Only LAW builders accept this form; generic HL7 builders still require a location.
    #[must_use]
    pub fn unlocated(code: &'a [u8], application_code: &'a [u8]) -> Self {
        Self {
            segment: b"",
            occurrence: 0,
            field: 0,
            code,
            application_code,
        }
    }
}
/// Builds a generic ACK only when requested; caller must choose the outcome explicitly.
/// Functional order/query responses belong to the later transaction layer.
/// # Errors
/// Rejects invalid control fields, unsafe positive codes, wrong modes and injected delimiters.
pub fn acknowledgment(
    message: &Message,
    response: Response<'_>,
    code: AckCode,
    responsibility: Responsibility,
    error: Option<AckError<'_>>,
    options: Options,
) -> Result<Option<Message>, Error> {
    acknowledgment_with_msh(
        Msh::new(message)?,
        response,
        code,
        responsibility,
        error,
        options,
        AckProfile::Base,
    )
}

/// Selects the identity length and wire-version policy owned by the ACK caller.
#[derive(Clone, Copy)]
pub(crate) enum AckProfile {
    /// General HL7 control constraints.
    Base,
    /// LAW's extended identities and compatible 2.5 version declarations.
    Law,
}

/// Renders an ACK with an already-validated control view and explicit profile.
pub(crate) fn acknowledgment_with_msh(
    msh: Msh<'_>,
    response: Response<'_>,
    code: AckCode,
    responsibility: Responsibility,
    error: Option<AckError<'_>>,
    options: Options,
    profile: AckProfile,
) -> Result<Option<Message>, Error> {
    let control_limit = match profile {
        AckProfile::Base => 20,
        AckProfile::Law => 50,
    };
    match (code, responsibility) {
        (AckCode::Aa, Responsibility::StoredAndInterpreted)
        | (AckCode::Ca, Responsibility::Stored | Responsibility::StoredAndInterpreted)
        | (
            AckCode::Ae | AckCode::Ar | AckCode::Ce | AckCode::Cr,
            Responsibility::NotStored
            | Responsibility::Stored
            | Responsibility::StoredAndInterpreted,
        ) => {}
        (AckCode::Aa, Responsibility::NotStored | Responsibility::Stored)
        | (AckCode::Ca, Responsibility::NotStored) => return Err(Error::NotStored),
    }
    let requested = match msh.mode() {
        AckMode::Original => {
            if code.accept() {
                return Err(Error::AckMode);
            }
            true
        }
        AckMode::Enhanced {
            accept,
            application,
        } => {
            let condition = if code.accept() { accept } else { application };
            condition.is_some_and(|condition| condition.requests(code.success()))
        }
    };
    if !requested {
        return Ok(None);
    }
    let delimiters = msh.segment().delimiters;
    let safe = |raw: &[u8], field, maximum| -> Result<(), Error> {
        if matches!(profile, AckProfile::Law) && field == 10 {
            return crate::law::control_id(raw).map(|_| ());
        }
        if raw.len() > maximum
            || raw
                .iter()
                .any(|b| !b.is_ascii_graphic() || delimiters.bytes().contains(b))
        {
            return Err(Error::Control {
                segment: "MSH",
                field,
            });
        }
        Ok(())
    };
    safe(response.control_id.raw(), 10, control_limit)?;
    safe(response.timestamp, 7, 26)?;
    if response.control_id.raw().is_empty() || response.control_id.raw() == msh.control_id() {
        return Err(Error::Control {
            segment: "MSH",
            field: 10,
        });
    }
    if response.timestamp.is_empty()
        || !response
            .timestamp
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-'))
    {
        return Err(Error::Control {
            segment: "MSH",
            field: 7,
        });
    }
    let field = delimiters.bytes().first().copied().ok_or(Error::Encoding)?;
    let component = delimiters.bytes().get(1).copied().ok_or(Error::Encoding)?;
    let segment = msh.segment();
    let get = |number| segment.field(number).map_or(&[][..], |f| f.raw());
    let mut message_type = b"ACK".to_vec();
    let trigger = msh.message_type().nth(1).unwrap_or_default();
    if !trigger.is_empty() {
        message_type.push(component);
        message_type.extend_from_slice(trigger);
    }
    if msh.version() == Version::V251 {
        message_type.push(component);
        message_type.extend_from_slice(b"ACK");
    }
    let mut raw = b"MSH".to_vec();
    for value in [
        get(2),
        get(5),
        get(6),
        get(3),
        get(4),
        response.timestamp,
        b"",
        &message_type,
        response.control_id.raw(),
        get(11),
        get(12),
        b"",
        b"",
        b"",
        b"",
        b"",
        get(18),
    ] {
        raw.push(field);
        raw.extend_from_slice(value);
    }
    raw.extend_from_slice(b"\rMSA");
    raw.push(field);
    raw.extend_from_slice(code.wire());
    raw.push(field);
    raw.extend_from_slice(msh.control_id());
    raw.push(b'\r');
    if let Some(error) = error {
        safe(error.segment, 0, 3)?;
        safe(error.code, 0, 3)?;
        safe(error.application_code, 0, 64)?;
        let unlocated = matches!(profile, AckProfile::Law)
            && error.segment.is_empty()
            && error.occurrence == 0
            && error.field == 0;
        if (!unlocated && (error.segment.len() != 3 || error.occurrence == 0 || error.field == 0))
            || error.code.is_empty()
        {
            return Err(Error::Control {
                segment: "ERR",
                field: 1,
            });
        }
        let mut location = Vec::new();
        if !unlocated {
            location.extend_from_slice(error.segment);
            location.push(component);
            location.extend_from_slice(error.occurrence.to_string().as_bytes());
            location.push(component);
            location.extend_from_slice(error.field.to_string().as_bytes());
        }
        raw.extend_from_slice(b"ERR");
        raw.push(field);
        match msh.version() {
            Version::V231 => {
                raw.extend_from_slice(&location);
                raw.push(component);
                raw.extend_from_slice(error.code);
            }
            Version::V251 => {
                raw.push(field);
                raw.extend_from_slice(&location);
                raw.push(field);
                raw.extend_from_slice(error.code);
                raw.push(field);
                raw.push(b'E');
                raw.push(field);
                raw.extend_from_slice(error.application_code);
            }
        }
        raw.push(b'\r');
    }
    if raw.len() > options.limits.message_bytes {
        return Err(Error::Limit {
            kind: "ack bytes",
            segment: 0,
            field: 0,
        });
    }
    // Output always uses standard CR, regardless of a negotiated inbound tolerance.
    let parse = match profile {
        AckProfile::Base => Message::parse,
        AckProfile::Law => Message::parse_law,
    };
    parse(
        &raw,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )
    .map(Some)
}
