//! Bounded host-owned sessions; no credential or CSRF token is logged.
use super::Token;
use analyte_model::Identifier;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) struct Session {
    pub(super) id: Token,
    pub(super) csrf: Token,
    pub(super) actor: Identifier,
    pub(super) authenticated: bool,
    created: Instant,
}

impl Session {
    pub(super) fn csrf_valid(&self, supplied: &str) -> bool {
        self.csrf.matches(supplied)
    }
}

#[derive(Default)]
pub(super) struct Sessions {
    // Protects capacity, expiry and session replacement; guards never cross await.
    entries: Mutex<Vec<Arc<Session>>>,
}

impl Sessions {
    pub(super) fn lookup(
        &self,
        cookie: Option<&str>,
    ) -> Result<Option<Arc<Session>>, crate::Error> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| crate::invalid("admin session lock poisoned"))?;
        entries.retain(|s| {
            s.created.elapsed() < Duration::from_secs(if s.authenticated { 3600 } else { 300 })
        });
        Ok(cookie.and_then(|cookie| entries.iter().find(|s| s.id.matches(cookie)).cloned()))
    }

    pub(super) fn issue(
        &self,
        authenticated: bool,
        replace: Option<&Session>,
    ) -> Result<Arc<Session>, crate::Error> {
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
            .map_err(|_| crate::invalid("admin session lock poisoned"))?;
        entries.retain(|s| {
            s.created.elapsed() < Duration::from_secs(if s.authenticated { 3600 } else { 300 })
                && replace.is_none_or(|old| !s.id.matches(old.id.form_value()))
        });
        if entries.len() >= 128 {
            if let Some(index) = entries.iter().position(|s| !s.authenticated) {
                entries.remove(index);
            } else {
                return Err(crate::invalid(
                    "admin session capacity reached; restart or wait for expiry",
                ));
            }
        }
        entries.push(session.clone());
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Plan sections 11/12: every session has its own CSRF token; authenticated
    // replacement invalidates the challenge, and live sessions cannot grow unbounded.
    #[test]
    fn plan12_csrf_session_isolation_replacement_and_capacity() -> Result<(), crate::Error> {
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
}
