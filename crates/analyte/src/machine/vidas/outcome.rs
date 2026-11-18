//! Works out the stored VIDAS transfer outcome from how the session ended.
use super::Error;
use crate::machine::driver::Trigger;
use crate::machine::receive::{custody::Refusal, error::Step};
use analyte_dialog::vidas::Timer;
use analyte_dialog::vidas::outcome::Code as VidasEnd;

/// A closed or cancelled connection names the outcome; otherwise the cause
/// that ended the session does, and a session without one is a transfer error.
pub(super) fn session_end(trigger: Trigger, result: &Result<(), Error>) -> VidasEnd {
    match trigger {
        Trigger::Cancelled => VidasEnd::TransferCancelled,
        Trigger::Disconnected => VidasEnd::TransferDisconnected,
        Trigger::Exchange | Trigger::Deadline => result
            .as_ref()
            .err()
            .and_then(failure_code)
            .unwrap_or(VidasEnd::TransferError),
    }
}

/// Maps each VIDAS failure to its stored code. In a pair of failures the later
/// one names the last step that ran, so it decides first.
fn failure_code(error: &Error) -> Option<VidasEnd> {
    match error {
        Error::ControlSuffixMismatch => Some(VidasEnd::ControlSuffixMismatch),
        Error::Frame(_) => Some(VidasEnd::FramingError),
        Error::ActiveDialogHasNoDeadline => Some(VidasEnd::InputFailed),
        Error::TransferUnreconciled { expired } => Some(match expired {
            Some(Timer::Acknowledgment) => VidasEnd::AcknowledgmentTimeout,
            Some(Timer::Interbyte) => VidasEnd::InterbyteTimeout,
            Some(Timer::Session) => VidasEnd::SessionTimeout,
            Some(Timer::Commit) => VidasEnd::CommitTimeout,
            None => VidasEnd::TransferRefusedOrUnexpected,
        }),
        Error::Refusal(Refusal::Evidence(cause)) => match cause.step() {
            Step::Write => Some(VidasEnd::WriteFailed),
            Step::Input => Some(VidasEnd::InputFailed),
            // A failed upload store parks the transfer; the park names the outcome.
            Step::Store { .. } | Step::Other => None,
        },
        Error::Refusal(Refusal::Cleanup(pair)) => {
            failure_code(&pair.effect).or_else(|| failure_code(&pair.original))
        }
        Error::Refusal(Refusal::Custody(_))
        | Error::OrderMapping(_)
        | Error::Orders(_)
        | Error::Dialog(_)
        | Error::Syntax(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::receive::custody::{StoreWaitError, WriteError, effect_failure};

    fn parked(expired: Option<Timer>) -> Error {
        Error::TransferUnreconciled { expired }
    }
    fn store_failure() -> Error {
        StoreWaitError::Operation(crate::IntakeError::UnknownReceipt).into()
    }
    fn park_failure() -> Error {
        crate::IntakeError::UnknownReceipt.into()
    }

    #[test]
    fn adr0004_vidas_write_failure_stores_write_failed() {
        let write = Error::from(WriteError::DeadlineOverflow);
        assert_eq!(
            session_end(Trigger::Exchange, &Err(write)),
            VidasEnd::WriteFailed
        );
        let uncertain = Error::from(WriteError::Deadline(analyte_transport::Error::Deadline(
            analyte_transport::DeadlineKind::WriteAll,
        )));
        assert_eq!(
            session_end(Trigger::Deadline, &Err(uncertain)),
            VidasEnd::WriteFailed
        );
    }

    #[test]
    fn adr0004_vidas_storage_failure_stores_the_park_outcome() {
        // A failed upload store parks without an expired deadline.
        let failure = effect_failure(Some(store_failure()), parked(None));
        assert_eq!(
            session_end(Trigger::Exchange, &Err(failure)),
            VidasEnd::TransferRefusedOrUnexpected
        );
        let late = effect_failure(Some(store_failure()), parked(Some(Timer::Commit)));
        assert_eq!(
            session_end(Trigger::Exchange, &Err(late)),
            VidasEnd::CommitTimeout
        );
        // A dialog that refuses the store failure ends before any park.
        let refused = effect_failure(
            Some(store_failure()),
            Error::from(analyte_dialog::vidas::Error::TimeReversed),
        );
        assert_eq!(
            session_end(Trigger::Exchange, &Err(refused)),
            VidasEnd::TransferError
        );
    }

    #[test]
    fn adr0004_vidas_timeouts_store_the_expired_deadline() {
        for (timer, code) in [
            (Timer::Acknowledgment, VidasEnd::AcknowledgmentTimeout),
            (Timer::Interbyte, VidasEnd::InterbyteTimeout),
            (Timer::Session, VidasEnd::SessionTimeout),
            (Timer::Commit, VidasEnd::CommitTimeout),
        ] {
            assert_eq!(
                session_end(Trigger::Deadline, &Err(parked(Some(timer)))),
                code
            );
            // Parking that fails keeps the park as the outcome.
            let failed = effect_failure(Some(parked(Some(timer))), park_failure());
            assert_eq!(session_end(Trigger::Deadline, &Err(failed)), code);
        }
    }

    #[test]
    fn adr0004_vidas_connection_end_outranks_the_session_result() {
        for result in [Ok(()), Err(parked(Some(Timer::Session)))] {
            assert_eq!(
                session_end(Trigger::Cancelled, &result),
                VidasEnd::TransferCancelled
            );
            assert_eq!(
                session_end(Trigger::Disconnected, &result),
                VidasEnd::TransferDisconnected
            );
        }
    }

    #[test]
    fn adr0004_vidas_framing_input_and_other_failures_keep_their_codes() {
        assert_eq!(
            session_end(Trigger::Exchange, &Err(Error::ControlSuffixMismatch)),
            VidasEnd::ControlSuffixMismatch
        );
        let framing = Error::from(analyte_framing::vidas::Error::Checksum);
        let failed = effect_failure(Some(framing), park_failure());
        assert_eq!(
            session_end(Trigger::Exchange, &Err(failed)),
            VidasEnd::FramingError
        );
        assert_eq!(
            session_end(Trigger::Exchange, &Err(Error::ActiveDialogHasNoDeadline)),
            VidasEnd::InputFailed
        );
        let input = Error::from(analyte_transport::Error::Offline);
        assert_eq!(
            session_end(Trigger::Exchange, &Err(input)),
            VidasEnd::InputFailed
        );
        assert_eq!(
            session_end(Trigger::Exchange, &Err(park_failure())),
            VidasEnd::TransferError
        );
        assert_eq!(
            session_end(Trigger::Exchange, &Ok(())),
            VidasEnd::TransferError
        );
    }
}
