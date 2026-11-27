//! Manages bounded operator sessions without logging credential or CSRF tokens.
use super::{
    token::Token,
    windows::{ACTOR_PREFIX, WindowsAccount},
};
use analyte_model::Identifier;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// How an admin-key session's audit actor begins; the command line refuses
/// actors that begin so.
pub(in crate::admin) const ADMIN_KEY_PREFIX: &str = "admin-session-";

/// Why the command line refused an `--actor`.
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    /// It begins like an actor only a sign-in on the admin pages records.
    #[error("{}", analyte_i18n::message("cli-error-reserved", [("value", (.0).to_string().into())]))]
    Reserved(&'static str),
    /// It is not an actor Analyte can record.
    #[error("{}", analyte_i18n::message("cli-error-text", [("value", (.0).to_string().into())]))]
    Text(#[from] analyte_model::TextError),
}

/// An actor typed on the command line, refused when it begins, in any case,
/// like an admin page sign-in's (`admin-session-`, `windows:`), so a typed
/// name can never pass as a verified one (ADR 0021 §5).
/// # Errors
/// [`ActorError::Reserved`] for such a beginning, [`ActorError::Text`] for
/// text that is not an [`Identifier`].
pub fn command_line_actor(text: &str) -> Result<Identifier, ActorError> {
    for prefix in [ADMIN_KEY_PREFIX, ACTOR_PREFIX] {
        let begins = text
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix));
        if begins {
            return Err(ActorError::Reserved(prefix));
        }
    }
    Ok(Identifier::new(text)?)
}

/// Who a session is signed in as. A signed-in session always names its
/// actor, so no session can be signed in without one.
pub(in crate::admin) enum SignedIn {
    /// A sign-in page was shown; nobody has signed in yet.
    Pending,
    /// Signed in with the admin key, recorded as `admin-session-<id>`.
    AdminKey {
        /// This session's audit actor.
        actor: Identifier,
    },
    /// Signed in as a Windows account, through a one-time code (ADR 0021).
    Windows(WindowsAccount),
}

impl SignedIn {
    /// A new admin-key sign-in with its own `admin-session-<id>` actor.
    pub(in crate::admin) fn admin_key() -> Result<Self, crate::admin::Error> {
        let actor = Identifier::new(&format!("{ADMIN_KEY_PREFIX}{}", uuid::Uuid::now_v7()))?;
        Ok(Self::AdminKey { actor })
    }

    /// How long a session signed in this way lives: 5 minutes for a sign-in
    /// page, 1 hour once signed in, whichever way.
    const fn lifetime(&self) -> Duration {
        match self {
            Self::Pending => Duration::from_secs(300),
            Self::AdminKey { .. } | Self::Windows(_) => Duration::from_secs(3600),
        }
    }
}

pub(in crate::admin) struct Session {
    pub(in crate::admin) id: Token,
    pub(in crate::admin) csrf: Token,
    signed: SignedIn,
    created: Instant,
}

impl Session {
    pub(in crate::admin) fn csrf_valid(&self, supplied: &str) -> bool {
        self.csrf.matches(supplied)
    }

    /// Whether someone has signed in on this session.
    pub(in crate::admin) fn is_signed_in(&self) -> bool {
        match self.signed {
            SignedIn::Pending => false,
            SignedIn::AdminKey { .. } | SignedIn::Windows(_) => true,
        }
    }

    /// The audit actor of what this session does; none before sign-in.
    pub(in crate::admin) fn actor(&self) -> Option<&Identifier> {
        match &self.signed {
            SignedIn::Pending => None,
            SignedIn::AdminKey { actor } => Some(actor),
            SignedIn::Windows(account) => Some(account.actor()),
        }
    }

    /// Who the sidebar says is signed in: the Windows account's name, or its
    /// SID; none for the admin key, which names nobody, and before sign-in.
    pub(in crate::admin) fn account(&self) -> Option<&str> {
        match &self.signed {
            SignedIn::Pending | SignedIn::AdminKey { .. } => None,
            SignedIn::Windows(account) => Some(account.shown()),
        }
    }

    fn fresh(&self) -> bool {
        self.created.elapsed() < self.signed.lifetime()
    }
}

/// The signed-in browser session, when the request has one; a request with
/// the admin key alone or a session not signed in yet has none.
pub(in crate::admin) fn signed_in(
    session: Option<&axum::Extension<Arc<Session>>>,
) -> Option<&Session> {
    session
        .map(|axum::Extension(session)| session.as_ref())
        .filter(|session| session.is_signed_in())
}

#[derive(Default)]
pub(in crate::admin) struct Sessions {
    // Protects capacity, expiry and session replacement; guards never cross await.
    entries: Mutex<Vec<Arc<Session>>>,
}

impl Sessions {
    pub(in crate::admin) fn lookup(
        &self,
        cookie: Option<&str>,
    ) -> Result<Option<Arc<Session>>, crate::admin::Error> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| crate::admin::Error::AdminSessionLockPoisoned)?;
        entries.retain(|s| s.fresh());
        Ok(cookie.and_then(|cookie| entries.iter().find(|s| s.id.matches(cookie)).cloned()))
    }

    /// Starts a session signed in as `signed`, ending `replace` if given.
    /// The same 128-session bound holds for every kind: sign-in pages give
    /// way first, signed-in sessions never.
    pub(in crate::admin) fn issue(
        &self,
        signed: SignedIn,
        replace: Option<&Session>,
    ) -> Result<Arc<Session>, crate::admin::Error> {
        let session = Arc::new(Session {
            id: Token::session()?,
            csrf: Token::session()?,
            signed,
            created: Instant::now(),
        });
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| crate::admin::Error::AdminSessionLockPoisoned)?;
        entries
            .retain(|s| s.fresh() && replace.is_none_or(|old| !s.id.matches(old.id.form_value())));
        if entries.len() >= 128 {
            if let Some(index) = entries.iter().position(|s| !s.is_signed_in()) {
                entries.remove(index);
            } else {
                return Err(crate::admin::Error::SessionCapacity);
            }
        }
        entries.push(session.clone());
        Ok(session)
    }

    /// Ends `session`: its cookie and CSRF token stop working at once.
    pub(in crate::admin) fn end(&self, session: &Session) -> Result<(), crate::admin::Error> {
        self.entries
            .lock()
            .map_err(|_| crate::admin::Error::AdminSessionLockPoisoned)?
            .retain(|s| !s.id.matches(session.id.form_value()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::windows::{AccountName, Sid};
    use super::*;

    fn windows() -> Result<SignedIn, crate::Error> {
        Ok(SignedIn::Windows(WindowsAccount::new(
            Sid::new("S-1-5-21-1-2-3-1001".into())?,
            Some(AccountName::new(r"LAB-PC\maria".into())?),
        )?))
    }

    // Plan sections 11/12: every session has its own CSRF token; a signed-in
    // replacement invalidates the challenge, and live sessions cannot grow
    // unbounded. ADR 0021 §4: Windows sessions share the same 128 bound.
    // Plan §12
    #[test]
    fn csrf_sessions_are_isolated_replaced_and_bounded() -> Result<(), crate::Error> {
        let sessions = Sessions::default();
        let first = sessions.issue(SignedIn::Pending, None)?;
        let second = sessions.issue(SignedIn::Pending, None)?;
        assert!(first.csrf_valid(first.csrf.form_value()));
        assert!(!first.csrf_valid(second.csrf.form_value()));
        assert!(!first.is_signed_in() && first.actor().is_none());
        let signed = sessions.issue(SignedIn::admin_key()?, Some(&first))?;
        assert!(sessions.lookup(Some(first.id.form_value()))?.is_none());
        assert!(
            sessions
                .lookup(Some(signed.id.form_value()))?
                .is_some_and(|s| s.is_signed_in())
        );
        for index in 0..127 {
            let kind = if index % 2 == 0 {
                windows()?
            } else {
                SignedIn::admin_key()?
            };
            sessions.issue(kind, None)?;
        }
        assert!(sessions.lookup(Some(second.id.form_value()))?.is_none());
        assert!(sessions.issue(windows()?, None).is_err());
        assert!(sessions.issue(SignedIn::admin_key()?, None).is_err());
        assert!(sessions.lookup(Some(signed.id.form_value()))?.is_some());
        Ok(())
    }

    // ADR 0021 §5: a key session is recorded as admin-session-<id> and names
    // nobody; a Windows session is recorded and named as its account.
    #[test]
    fn sessions_name_their_actor() -> Result<(), crate::Error> {
        let sessions = Sessions::default();
        let key = sessions.issue(SignedIn::admin_key()?, None)?;
        assert!(
            key.actor()
                .is_some_and(|a| a.as_str().starts_with(ADMIN_KEY_PREFIX))
        );
        assert_eq!(key.account(), None);
        let maria = sessions.issue(windows()?, None)?;
        assert_eq!(
            maria.actor().map(Identifier::as_str),
            Some(r"windows:S-1-5-21-1-2-3-1001:LAB-PC\maria")
        );
        assert_eq!(maria.account(), Some(r"LAB-PC\maria"));
        Ok(())
    }

    // ADR 0021 §5: the command line refuses actors spelled like an admin
    // page sign-in, in any case, and keeps every other actor as typed.
    #[test]
    fn command_line_refuses_sign_in_actors() {
        for reserved in [
            "admin-session-1",
            r"windows:S-1-5-21-1-2-3-1001:LAB-PC\maria",
            "Windows:S-1-5-18",
            "ADMIN-SESSION-x",
        ] {
            assert!(
                matches!(command_line_actor(reserved), Err(ActorError::Reserved(_))),
                "{reserved}"
            );
        }
        assert!(matches!(
            command_line_actor("night\nshift"),
            Err(ActorError::Text(_))
        ));
        assert_eq!(
            command_line_actor("night-shift")
                .map(|a| a.as_str().to_owned())
                .ok(),
            Some("night-shift".into())
        );
        assert!(command_line_actor("windows").is_ok());
    }

    // Plan §12 and ADR 0021 §4: a sign-in page lasts 5 minutes, a signed-in
    // session 1 hour, whether signed in with the key or with Windows.
    #[test]
    fn expired_challenges_and_sessions_are_refused() -> Result<(), crate::Error> {
        for (signed, lifetime) in [
            (SignedIn::Pending, 300),
            (SignedIn::admin_key()?, 3600),
            (windows()?, 3600),
        ] {
            let sessions = Sessions::default();
            let mut session = sessions.issue(signed, None)?;
            let cookie = session.id.form_value().to_owned();
            for (age, kept) in [(lifetime - 1, true), (lifetime, false)] {
                sessions.entries.lock().map_err(|_| "session lock")?.clear();
                Arc::get_mut(&mut session)
                    .ok_or("unexpected session owner")?
                    .created = Instant::now()
                    .checked_sub(Duration::from_secs(age))
                    .ok_or("clock range")?;
                sessions
                    .entries
                    .lock()
                    .map_err(|_| "session lock")?
                    .push(session.clone());
                let found = sessions.lookup(Some(&cookie))?;
                assert_eq!(found.is_some(), kept, "{lifetime} s, aged {age} s");
            }
            assert!(
                sessions
                    .entries
                    .lock()
                    .map_err(|_| "session lock")?
                    .is_empty()
            );
        }
        Ok(())
    }
}
