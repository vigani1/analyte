//! Works out the stored Autolyser transfer outcome from how the session ended.
use super::Error;
use crate::machine::driver::Trigger;
use crate::machine::receive::{custody::Refusal, error::Step};
use crate::orders::outcome::{AutolyserEnd, Interruption, Transfer};

/// The cause that ended the session names the outcome; a session without a
/// named cause takes it from the input it was handling.
pub(super) fn end_code(trigger: Trigger, result: &Result<(), Error>) -> AutolyserEnd {
    result
        .as_ref()
        .err()
        .and_then(failure_code)
        .unwrap_or_else(|| {
            interruption(trigger).map_or(AutolyserEnd::TransferError, Interruption::end)
        })
}

/// A parked reply cut short by a deadline or a closed connection keeps that
/// interruption in its code; any other park keeps the dialog's reason.
pub(super) fn parked(trigger: Trigger, error: &analyte_dialog::autolyser::Error) -> Transfer {
    match interruption(trigger) {
        Some(interruption) if matches!(error, analyte_dialog::autolyser::Error::Incomplete) => {
            Transfer::AutolyserIncomplete(interruption)
        }
        Some(_) | None => Transfer::AutolyserParked(error.clone()),
    }
}

/// An exchange is no interruption; every other input cut the transfer short.
const fn interruption(trigger: Trigger) -> Option<Interruption> {
    match trigger {
        Trigger::Exchange => None,
        Trigger::Deadline => Some(Interruption::Timeout),
        Trigger::Disconnected => Some(Interruption::Disconnected),
        Trigger::Cancelled => Some(Interruption::Cancelled),
    }
}

/// Maps each Autolyser failure to its stored code. In a pair of failures the
/// later one names the last step that ran, so it decides first.
fn failure_code(error: &Error) -> Option<AutolyserEnd> {
    match error {
        Error::Refusal(Refusal::Evidence(cause)) => match cause.step() {
            Step::Write => Some(AutolyserEnd::WriteFailed),
            Step::Input => Some(AutolyserEnd::InputFailed),
            Step::Store { timed_out: true } => Some(AutolyserEnd::StorageTimeout),
            Step::Store { timed_out: false } => Some(AutolyserEnd::StorageFailed),
            Step::Other => None,
        },
        Error::Refusal(Refusal::Cleanup(pair)) => {
            failure_code(&pair.effect).or_else(|| failure_code(&pair.original))
        }
        Error::Refusal(Refusal::Custody(_))
        | Error::PollCustodyUnreconciled
        | Error::PollUnreconciled
        | Error::ReportReceiptMissing
        | Error::OrderMapping(_)
        | Error::Orders(_)
        | Error::Dialog(_)
        | Error::Syntax(_)
        | Error::Frame(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::receive::custody::{StoreWaitError, WriteError, effect_failure};
    use std::time::Duration;

    fn dialog(error: analyte_dialog::autolyser::Error) -> Error {
        error.into()
    }

    #[test]
    fn adr0004_autolyser_write_failure_stores_write_failed() {
        for trigger in [Trigger::Exchange, Trigger::Deadline] {
            let write = Error::from(WriteError::DeadlineOverflow);
            assert_eq!(end_code(trigger, &Err(write)), AutolyserEnd::WriteFailed);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn adr0004_autolyser_storage_failure_stores_failed_or_timeout() {
        let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
            .await
            .unwrap_err();
        let timeout = Error::from(StoreWaitError::<crate::IntakeError>::Timeout(elapsed));
        // A store that fails while the connection closes still names the store.
        assert_eq!(
            end_code(Trigger::Disconnected, &Err(timeout)),
            AutolyserEnd::StorageTimeout
        );
        let failed = || {
            Error::from(StoreWaitError::Operation(
                crate::IntakeError::UnknownReceipt,
            ))
        };
        assert_eq!(
            end_code(Trigger::Exchange, &Err(failed())),
            AutolyserEnd::StorageFailed
        );
        // The dialog's refusal of the failed store and a failed park keep the store.
        for effect in [
            dialog(analyte_dialog::autolyser::Error::TimeReversed),
            crate::IntakeError::UnknownReceipt.into(),
        ] {
            let pair = effect_failure(Some(failed()), effect);
            assert_eq!(
                end_code(Trigger::Deadline, &Err(pair)),
                AutolyserEnd::StorageFailed
            );
        }
    }

    #[test]
    fn adr0004_autolyser_timeout_stores_transfer_timeout() {
        assert_eq!(
            end_code(Trigger::Deadline, &Ok(())),
            AutolyserEnd::TransferTimeout
        );
        let refused = dialog(analyte_dialog::autolyser::Error::Busy);
        assert_eq!(
            end_code(Trigger::Deadline, &Err(refused)),
            AutolyserEnd::TransferTimeout
        );
    }

    #[test]
    fn adr0004_autolyser_session_end_follows_its_input() {
        for (trigger, code) in [
            (Trigger::Exchange, AutolyserEnd::TransferError),
            (Trigger::Disconnected, AutolyserEnd::TransferDisconnected),
            (Trigger::Cancelled, AutolyserEnd::TransferCancelled),
        ] {
            assert_eq!(end_code(trigger, &Ok(())), code);
            assert_eq!(end_code(trigger, &Err(Error::ReportReceiptMissing)), code);
        }
        let input = Error::from(analyte_transport::Error::Offline);
        assert_eq!(
            end_code(Trigger::Exchange, &Err(input)),
            AutolyserEnd::InputFailed
        );
    }

    #[test]
    fn adr0004_autolyser_parked_replies_keep_their_interruption() {
        use analyte_dialog::autolyser::Error as Dialog;
        for (trigger, expected) in [
            (Trigger::Deadline, "Autolyser-transfer-timeout-incomplete"),
            (
                Trigger::Disconnected,
                "Autolyser-transfer-disconnected-incomplete",
            ),
            (
                Trigger::Cancelled,
                "Autolyser-transfer-cancelled-incomplete",
            ),
            (Trigger::Exchange, "Autolyser-parked-incomplete"),
        ] {
            assert_eq!(parked(trigger, &Dialog::Incomplete).to_string(), expected);
        }
        assert_eq!(
            parked(Trigger::Deadline, &Dialog::Ack).to_string(),
            "Autolyser-parked-ack"
        );
    }
}
