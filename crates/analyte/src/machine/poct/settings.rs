//! Translates installed POCT syntax and dialog bounds.
use super::super::custody::ms;
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

pub(crate) fn dialog(settings: &analyte_config::PoctStack) -> analyte_dialog::poct::Config {
    let policy = settings.policy();
    let limits = limits(settings);
    analyte_dialog::poct::Config {
        depth: policy.depth,
        pending_messages: policy.pending_messages,
        message_bytes: limits.message_bytes,
        response_timeout: ms(policy.response_ms),
        unannounced_ack_timeout: ms(policy.acknowledgment_ms),
    }
}
