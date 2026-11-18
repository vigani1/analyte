//! Translates installed LAW syntax settings into parser options.
/// The site's time zone, when configured, lets a received MSH-7 omit its offset.
pub(crate) fn options(
    settings: &analyte_config::LawStack,
    site: &analyte_config::Site,
) -> analyte_hl7::Options {
    analyte_hl7::Options {
        header_zone: if site.time_zone.is_some() {
            analyte_hl7::HeaderZone::Site
        } else {
            analyte_hl7::HeaderZone::Required
        },
        ..super::super::hl7::settings::options_from_values(
            analyte_config::Encoding::Utf8,
            analyte_config::Terminator::Cr,
            analyte_config::Escapes::Reject,
            settings.settings().limits(),
        )
    }
}
pub(crate) fn dialog(
    settings: &analyte_config::LawStack,
    site: &analyte_config::Site,
    link: analyte_dialog::law::Link,
) -> Result<analyte_dialog::law::Dialog, super::Error> {
    let mut options = options(settings, site);
    options.limits.message_bytes = options
        .limits
        .message_bytes
        .min(settings.settings().frame_limit());
    Ok(analyte_dialog::law::Dialog::new(
        options,
        std::time::Duration::from_millis(u64::from(
            settings.settings().timeouts().receive_timeout_ms(),
        )),
        std::time::Duration::from_millis(u64::from(
            settings.settings().timeouts().send_timeout_ms(),
        )),
        link,
    )?)
}

pub(crate) fn frame_limit(
    settings: &analyte_config::LawStack,
) -> Result<analyte_framing::hl7::Limit, analyte_framing::hl7::Error> {
    analyte_framing::hl7::Limit::new(settings.settings().frame_limit())
}
