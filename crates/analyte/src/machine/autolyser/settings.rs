//! Translates installed autolyser dialog settings.
use super::super::custody::ms;

pub(crate) fn dialog(
    settings: &analyte_config::AutolyserStack,
) -> analyte_dialog::autolyser::Config {
    let policy = settings.policy();
    analyte_dialog::autolyser::Config {
        poll: ms(policy.poll_ms),
        ack: ms(policy.acknowledgment_ms),
        response: ms(policy.response_ms),
        inter_byte: ms(policy.interbyte_ms),
        storage: ms(policy.storage_ms),
        max_bytes: settings.settings().frame_limit(),
        controls: policy.controls,
        recovery: if policy.recover_last_on_poll {
            analyte_dialog::autolyser::Recovery::LastOnPoll
        } else {
            analyte_dialog::autolyser::Recovery::ManualLast
        },
    }
}
