//! The one-time codes the sign-in pipe hands out and `GET /login/windows` takes back (ADR 0021 §4): single use, 20 s, at most 8 waiting, in memory only, so a restart clears them.
use super::{
    WindowsAccount,
    line::{Code, Next},
};
use crate::admin::{Token, TokenError};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long a code works after it was handed out.
const LIFETIME: Duration = Duration::from_secs(20);
/// The most codes waiting at once.
const PENDING: usize = 8;

/// One code waiting to be taken back, with whom and where it signs in.
struct Kept {
    code: Token,
    account: WindowsAccount,
    next: Next,
    made: Instant,
}

impl Kept {
    fn fresh(&self) -> bool {
        self.made.elapsed() < LIFETIME
    }
}

/// The codes waiting to be taken back.
#[derive(Default)]
pub(in crate::admin) struct Codes {
    // Protects the 8-code bound and single use: a code is found and removed
    // under one guard. Guards never cross an await.
    kept: Mutex<Vec<Kept>>,
}

/// Why no code was handed out or taken back.
#[derive(Debug, thiserror::Error)]
pub(in crate::admin) enum CodeError {
    /// [`PENDING`] codes are already waiting.
    #[error("8 Windows sign-in codes are already waiting")]
    Full,
    /// The OS random source failed.
    #[error("the Windows sign-in code could not be made: {0}")]
    Random(#[source] TokenError),
    /// A thread panicked while holding the table.
    #[error("the Windows sign-in code table lock is poisoned")]
    Poisoned,
}

impl Codes {
    /// A new code that signs `account` in and opens `next`, valid for 20 s.
    /// # Errors
    /// [`CodeError`] when 8 codes are waiting, no random code could be made
    /// or the table is unusable.
    pub(in crate::admin) fn keep(
        &self,
        account: WindowsAccount,
        next: Next,
    ) -> Result<Code, CodeError> {
        let code = Token::session().map_err(CodeError::Random)?;
        let text = code.form_value().to_owned();
        let mut kept = self.kept.lock().map_err(|_| CodeError::Poisoned)?;
        kept.retain(Kept::fresh);
        if kept.len() >= PENDING {
            return Err(CodeError::Full);
        }
        kept.push(Kept {
            code,
            account,
            next,
            made: Instant::now(),
        });
        Ok(Code::new(text))
    }

    /// Takes `supplied` back: the account and page of a fresh code that
    /// matches, compared in constant time, removed under the same guard so it
    /// works once; none for a used, expired or unknown code.
    /// # Errors
    /// [`CodeError::Poisoned`] when the table is unusable.
    pub(in crate::admin) fn redeem(
        &self,
        supplied: &str,
    ) -> Result<Option<(WindowsAccount, Next)>, CodeError> {
        let mut kept = self.kept.lock().map_err(|_| CodeError::Poisoned)?;
        kept.retain(Kept::fresh);
        let found = kept.iter().position(|kept| kept.code.matches(supplied));
        Ok(found.map(|index| {
            let taken = kept.swap_remove(index);
            (taken.account, taken.next)
        }))
    }

    /// Ages every waiting code by `by`, as if that time had passed.
    #[cfg(test)]
    pub(in crate::admin) fn age(&self, by: Duration) -> Result<(), &'static str> {
        let mut kept = self.kept.lock().map_err(|_| "code table poisoned")?;
        for code in kept.iter_mut() {
            code.made = code.made.checked_sub(by).ok_or("clock range")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AccountName, Sid};
    use super::*;

    type Failure = Box<dyn std::error::Error>;

    fn maria() -> Result<WindowsAccount, Failure> {
        Ok(WindowsAccount::new(
            Sid::new("S-1-5-21-1-2-3-1001".into())?,
            Some(AccountName::new("LAB-PC\\maria".into())?),
        )?)
    }

    // ADR 0021 §4: a code opens its account and page once; a second use, a
    // wrong code and an empty one open nothing.
    #[test]
    fn code_works_once() -> Result<(), Failure> {
        let codes = Codes::default();
        let code = codes.keep(maria()?, Next::new("/parked/12"))?;
        assert!(codes.redeem("")?.is_none());
        assert!(codes.redeem(&"A".repeat(44))?.is_none());
        let (account, next) = codes.redeem(code.text())?.ok_or("code refused")?;
        assert_eq!(account, maria()?);
        assert_eq!(next.as_str(), "/parked/12");
        assert!(codes.redeem(code.text())?.is_none());
        Ok(())
    }

    // ADR 0021 §4: a code expires 20 s after it was handed out.
    #[test]
    fn code_expires_after_twenty_seconds() -> Result<(), Failure> {
        let codes = Codes::default();
        let young = codes.keep(maria()?, Next::new("/"))?;
        codes.age(LIFETIME - Duration::from_secs(1))?;
        assert!(codes.redeem(young.text())?.is_some());
        let old = codes.keep(maria()?, Next::new("/"))?;
        codes.age(LIFETIME)?;
        assert!(codes.redeem(old.text())?.is_none());
        Ok(())
    }

    // ADR 0021 §4: at most 8 codes wait; expired ones make room again.
    #[test]
    fn at_most_eight_codes_wait() -> Result<(), Failure> {
        let codes = Codes::default();
        let first = codes.keep(maria()?, Next::new("/"))?;
        for _ in 1..PENDING {
            codes.keep(maria()?, Next::new("/"))?;
        }
        assert!(matches!(
            codes.keep(maria()?, Next::new("/")),
            Err(CodeError::Full)
        ));
        assert!(codes.redeem(first.text())?.is_some());
        codes.keep(maria()?, Next::new("/"))?;
        codes.age(LIFETIME)?;
        codes.keep(maria()?, Next::new("/"))?;
        Ok(())
    }
}
