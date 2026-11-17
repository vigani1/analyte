//! Reads POCT observations with durable source Hello context and installed bounds.
use super::Error;
pub(super) fn read(
    input: analyte_mapping::Input<'_>,
    raw: &[u8],
    settings: &analyte_config::PoctStack,
    hello: Option<&[u8]>,
) -> Result<analyte_mapping::Output, Error> {
    let hello = hello
        .map(|raw| analyte_poct::Message::parse(raw, limits(settings)))
        .transpose()?;
    let device = hello.as_ref().and_then(|hello| match hello.body() {
        analyte_poct::Body::Hello { device_id, .. } => Some(device_id.as_str()),
        analyte_poct::Body::Status { .. }
        | analyte_poct::Body::PatientObservations(_)
        | analyte_poct::Body::NonPatientObservations(_)
        | analyte_poct::Body::EndTopic(_)
        | analyte_poct::Body::Terminate(_)
        | analyte_poct::Body::Acknowledgement { .. }
        | analyte_poct::Body::Escape { .. }
        | analyte_poct::Body::KeepAlive
        | analyte_poct::Body::Request(_) => None,
    });
    let message = analyte_poct::Message::parse(raw, limits(settings))?;
    Ok(analyte_mapping::poct::map(
        settings,
        &message,
        analyte_mapping::Input { device, ..input },
    )?)
}
/// Translates installed POCT bounds for both live syntax checks and retained receipts.
#[must_use]
pub(crate) fn limits(settings: &analyte_config::PoctStack) -> analyte_poct::Limits {
    let policy = &settings.policy();
    analyte_poct::Limits {
        message_bytes: settings
            .settings()
            .frame_limit()
            .min(settings.settings().limits().bytes()),
        depth: policy.depth,
        elements: policy.elements,
        attribute_bytes: policy.attribute_bytes,
    }
}
