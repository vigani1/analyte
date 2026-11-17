//! Selects receipt framing from installed configuration without guessing from content.
use super::Error;
use analyte_config::{RecordTransport, Stack, Validated};
use std::borrow::Cow;
/// Returns exact syntax bytes from the configured stored receipt envelope.
/// # Errors
/// Returns framing and assembly defects without parsing a partial message.
pub(crate) fn payload<'a>(config: &Validated, raw: &'a [u8]) -> Result<Cow<'a, [u8]>, Error> {
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Folder(_))
        | Stack::Hl7(RecordTransport::Folder(_))
        | Stack::Poct(_)
        | Stack::Vidas(_)
        | Stack::Autolyser(_) => Ok(raw.into()),
        Stack::Astm(RecordTransport::Stream(s)) => Ok(analyte_framing::astm::decode(
            raw,
            crate::machine::astm::settings::frame_limit(s)?,
            crate::machine::astm::settings::checksum_policy(s),
            crate::machine::astm::settings::record_boundaries(s),
            s.settings().syntax().limits().bytes(),
        )?
        .into()),
        Stack::Hl7(RecordTransport::Stream(s)) => Ok(analyte_framing::hl7::decode(
            raw,
            crate::machine::hl7::settings::frame_limit(s)?,
        )?
        .into()),
        Stack::Law(s) => Ok(analyte_framing::hl7::decode(
            raw,
            crate::machine::law::settings::frame_limit(s)?,
        )?
        .into()),
    }
}
