//! Manages bounded operator sessions without logging credential or CSRF tokens.
use super::token::Token;
use analyte_model::Identifier;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(in crate::admin) struct Session {
    pub(in crate::admin) id: Token,
    pub(in crate::admin) csrf: Token,
    pub(in crate::admin) actor: Identifier,
    pub(in crate::admin) authenticated: bool,
    created: Instant,
}

impl Session {
    pub(in crate::admin) fn csrf_valid(&self, supplied: &str) -> bool {
        self.csrf.matches(supplied)
    }
}

/// The signed-in browser session, when the request has one; a request with
/// the admin key alone or a session not signed in yet has none.
pub(in crate::admin) fn signed_in(
    session: Option<&axum::Extension<Arc<Session>>>,
) -> Option<&Session> {
    session
        .map(|axum::Extension(session)| session.as_ref())
        .filter(|session| session.authenticated)
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
        entries.retain(|s| {
            s.created.elapsed() < Duration::from_secs(if s.authenticated { 3600 } else { 300 })
        });
        Ok(cookie.and_then(|cookie| entries.iter().find(|s| s.id.matches(cookie)).cloned()))
    }

    pub(in crate::admin) fn issue(
        &self,
        authenticated: bool,
        replace: Option<&Session>,
    ) -> Result<Arc<Session>, crate::admin::Error> {
        let session = Arc::new(Session {
            id: Token::session()?,
            csrf: Token::session()?,
            actor: Identifier::new(&format!("admin-session-{}", uuid::Uuid::now_v7()))?,
            authenticated,
            created: Instant::now(),
        });
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| crate::admin::Error::AdminSessionLockPoisoned)?;
        entries.retain(|s| {
            s.created.elapsed() < Duration::from_secs(if s.authenticated { 3600 } else { 300 })
                && replace.is_none_or(|old| !s.id.matches(old.id.form_value()))
        });
        if entries.len() >= 128 {
            if let Some(index) = entries.iter().position(|s| !s.authenticated) {
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
    use super::*;

    // Plan sections 11/12: every session has its own CSRF token; authenticated
    // replacement invalidates the challenge, and live sessions cannot grow unbounded.
    // Plan §12
    #[test]
    fn csrf_sessions_are_isolated_replaced_and_bounded() -> Result<(), crate::Error> {
        let sessions = Sessions::default();
        let first = sessions.issue(false, None)?;
        let second = sessions.issue(false, None)?;
        assert!(first.csrf_valid(first.csrf.form_value()));
        assert!(!first.csrf_valid(second.csrf.form_value()));
        let authenticated = sessions.issue(true, Some(&first))?;
        assert!(sessions.lookup(Some(first.id.form_value()))?.is_none());
        assert!(
            sessions
                .lookup(Some(authenticated.id.form_value()))?
                .is_some_and(|s| s.authenticated)
        );
        for _ in 0..127 {
            sessions.issue(true, None)?;
        }
        assert!(sessions.lookup(Some(second.id.form_value()))?.is_none());
        assert!(sessions.issue(true, None).is_err());
        assert!(
            sessions
                .lookup(Some(authenticated.id.form_value()))?
                .is_some()
        );
        Ok(())
    }
    // Plan §12
    #[test]
    fn expired_challenges_and_sessions_are_refused() -> Result<(), crate::Error> {
        for (authenticated, lifetime) in [(false, 300), (true, 3600)] {
            let sessions = Sessions::default();
            let mut session = sessions.issue(authenticated, None)?;
            let cookie = session.id.form_value().to_owned();
            assert!(sessions.lookup(Some(&cookie))?.is_some());
            sessions.entries.lock().map_err(|_| "session lock")?.clear();
            Arc::get_mut(&mut session)
                .ok_or("unexpected session owner")?
                .created = Instant::now()
                .checked_sub(Duration::from_secs(lifetime))
                .ok_or("clock range")?;
            sessions
                .entries
                .lock()
                .map_err(|_| "session lock")?
                .push(session);
            assert!(sessions.lookup(Some(&cookie))?.is_none());
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
