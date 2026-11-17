//! Builds LAW ACK/R22 result acknowledgments on top of the core HL7 acknowledgment.
use crate::encode::wire::Wire;
use crate::law::{Error, content};
use crate::{AckCode, AckError, Message, Options, Response, Responsibility};

/// Renders LAW ACK/R22 through the core acknowledgment function and adds its profile.
/// # Errors
/// Rejects unsafe positive responsibility, metadata, missing error details or output limits.
pub fn result_acknowledgment(
    message: &Message,
    response: Response<'_>,
    code: AckCode,
    responsibility: Responsibility,
    error: Option<AckError<'_>>,
    options: Options,
) -> Result<Option<Message>, Error> {
    if !crate::law::profile::timestamp(response.timestamp, true) {
        return Err(content("MSH", 7));
    }
    if code == AckCode::Aa {
        crate::law::Results::new(message)?;
    }
    if (code == AckCode::Aa) == error.is_some() {
        return Err(content("ERR", 0));
    }
    let Some(ack) = crate::encode::ack::acknowledgment_with_msh(
        crate::law::msh(message)?,
        response,
        code,
        responsibility,
        error,
        options,
        crate::encode::ack::AckProfile::Law,
    )?
    else {
        return Ok(None);
    };
    let mut wire = Wire::new(options.limits.message_bytes, message.delimiters().field);
    let msh = ack.segments().next().ok_or(content("MSH", 0))?;
    wire.append(b"MSH")?;
    let mut profile = b"LAB-29".to_vec();
    profile.push(message.delimiters().component);
    profile.extend_from_slice(b"IHE");
    for field in 2..=21 {
        wire.field(if field == 21 {
            &profile
        } else {
            crate::law::envelope::reply_field(msh, field)
        })?;
    }
    wire.append(b"\r")?;
    super::append_ack_segments(&mut wire, &ack)?;
    let output = crate::law::parse_message(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?;
    crate::law::validate_control(&output, crate::law::ControlDirection::Send)?;
    Ok(Some(output))
}
