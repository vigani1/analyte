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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Elapsed time decreased.
    Reversed,
    /// A required future deadline cannot be represented.
    Exhausted,
}

impl Clock {
    /// Returns the last accepted elapsed time.
    #[must_use]
    pub fn now(&self) -> Duration {
        self.now
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
