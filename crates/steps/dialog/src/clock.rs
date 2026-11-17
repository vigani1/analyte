//! Tracks monotonic connection time without reading the operating system clock.
use std::time::Duration;

/// A connection-local logical clock, starting at zero.
/// Hosts supply elapsed monotonic time to every event, including disconnect and
/// storage completion. Dialogs expire due work before accepting new input and
/// report an absolute `next_deadline`; hosts wait for it, input or cancellation.
/// A timeout never cancels durable work or authorizes replay.
#[derive(Debug, Default)]
pub struct Clock {
    now: Duration,
}

/// Time cannot safely be applied to the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Elapsed time decreased.
    #[error("monotonic time reversed")]
    Reversed,
    /// A required future deadline cannot be represented.
    #[error("deadline cannot be represented")]
    Exhausted,
}

impl Clock {
    /// Returns the last accepted elapsed time.
    #[must_use]
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Computes a future deadline from the accepted connection time.
    /// # Errors
    /// Returns `Exhausted` if the deadline cannot be represented.
    pub fn after(&self, wait: Duration) -> Result<Duration, Error> {
        self.now.checked_add(wait).ok_or(Error::Exhausted)
    }

    /// Advances only when time is monotonic and the required headroom fits.
    /// # Errors
    /// Rejects reversed or overflowing time without changing the clock.
    pub fn advance(&mut self, now: Duration, headroom: Duration) -> Result<(), Error> {
        if now < self.now {
            return Err(Error::Reversed);
        }
        now.checked_add(headroom).ok_or(Error::Exhausted)?;
        self.now = now;
        Ok(())
    }
}

impl Error {
    /// Preserves a dialog's public time-error variants through one conversion.
    pub(crate) fn into_dialog<T>(self, reversed: T, exhausted: T) -> T {
        match self {
            Self::Reversed => reversed,
            Self::Exhausted => exhausted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr_0001_clock_rejection_preserves_accepted_time() {
        let mut clock = Clock::default();
        clock
            .advance(Duration::from_secs(2), Duration::ZERO)
            .unwrap();
        assert_eq!(
            clock.advance(Duration::from_secs(1), Duration::ZERO),
            Err(Error::Reversed)
        );
        assert_eq!(clock.now(), Duration::from_secs(2));
        assert_eq!(clock.after(Duration::MAX), Err(Error::Exhausted));
        assert_eq!(
            clock.after(Duration::from_secs(3)),
            Ok(Duration::from_secs(5))
        );
        assert_eq!(Error::Reversed.into_dialog(1, 2), 1);
        assert_eq!(Error::Exhausted.into_dialog(1, 2), 2);
    }
}
