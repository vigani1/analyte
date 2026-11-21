//! Who may sign in with Windows: the decision from a token's groups (ADR 0021 §3), the same on every platform.

/// `BUILTIN\Administrators`.
pub(super) const ADMINISTRATORS: &str = "S-1-5-32-544";
/// winnt.h `SE_GROUP_ENABLED`: the group counts for access checks.
const ENABLED: u32 = 0x4;
/// winnt.h `SE_GROUP_USE_FOR_DENY_ONLY`: the group only denies access, as
/// Administrators does in an administrator's normal (not elevated) token.
const DENY_ONLY: u32 = 0x10;
/// Canonical SID text is far shorter (ADR 0017's read bound).
const SID_LIMIT: usize = 256;

/// A security identifier in canonical text form, such as `S-1-5-32-544`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct Sid(String);

/// Why text is not a canonical SID.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a canonical SID")]
pub(in crate::admin) struct NotSid;

impl Sid {
    /// Accepts `S-1-` then dash-separated decimal numbers, at most 256 bytes.
    /// # Errors
    /// [`NotSid`] for anything else.
    pub(in crate::admin) fn new(text: String) -> Result<Self, NotSid> {
        let parts = text.strip_prefix("S-1-").ok_or(NotSid)?;
        let numeric = parts
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
        if numeric && text.len() <= SID_LIMIT {
            Ok(Self(text))
        } else {
            Err(NotSid)
        }
    }

    /// The canonical text.
    pub(in crate::admin) fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a group takes part in a token's access checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Membership {
    /// It grants access.
    Enabled,
    /// It can only deny access.
    DenyOnly,
    /// Present but switched off.
    Disabled,
}

impl Membership {
    /// Reads a token group's `SE_GROUP_*` bits; deny-only wins, since Windows
    /// never lets a deny-only group grant.
    pub(in crate::admin) fn of(attributes: u32) -> Self {
        if attributes & DENY_ONLY != 0 {
            Self::DenyOnly
        } else if attributes & ENABLED != 0 {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// One group in a caller's token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct Group {
    /// The group's SID.
    pub(in crate::admin) sid: Sid,
    /// How it counts.
    pub(in crate::admin) membership: Membership,
}

/// The rule the owner set on 2026-11-21: members of the local "Analyte users"
/// group, enabled in the token, and Administrators, enabled or deny-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct Rule {
    /// The local group's SID; none when the group does not exist, so only
    /// Administrators may sign in.
    users: Option<Sid>,
    /// Always true outside tests: the test runner is an administrator, so
    /// refusals can only be shown with Administrators left out.
    administrators: bool,
}

impl Rule {
    /// The rule with the "Analyte users" group's SID, when it exists.
    pub(in crate::admin) fn new(users: Option<Sid>) -> Self {
        Self {
            users,
            administrators: true,
        }
    }

    /// Only `users`, without Administrators: a test that runs as an
    /// administrator can then be refused.
    #[cfg(test)]
    pub(in crate::admin) fn without_administrators(users: Sid) -> Self {
        Self {
            users: Some(users),
            administrators: false,
        }
    }
}

/// Whether a caller may sign in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Decision {
    /// A group in the rule counts for it.
    Allowed,
    /// No group in the rule counts for it.
    NotAllowed,
}

/// Decides from the token's groups alone; the user SID never counts.
pub(in crate::admin) fn allowed(groups: &[Group], rule: &Rule) -> Decision {
    let counts = |group: &Group| match group.membership {
        Membership::Enabled => {
            rule.users.as_ref() == Some(&group.sid)
                || (rule.administrators && group.sid.as_str() == ADMINISTRATORS)
        }
        // UAC is not a security boundary: an unelevated administrator still counts.
        Membership::DenyOnly => rule.administrators && group.sid.as_str() == ADMINISTRATORS,
        Membership::Disabled => false,
    };
    if groups.iter().any(counts) {
        Decision::Allowed
    } else {
        Decision::NotAllowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USERS: &str = "S-1-5-21-1-2-3-1009";
    const EVERYONE: &str = "S-1-1-0";

    fn sid(text: &str) -> Sid {
        Sid(text.to_owned())
    }

    fn group(text: &str, attributes: u32) -> Group {
        Group {
            sid: sid(text),
            membership: Membership::of(attributes),
        }
    }

    fn decide(groups: &[Group]) -> Decision {
        allowed(groups, &Rule::new(Some(sid(USERS))))
    }

    // ADR 0021 §3: a token whose groups name neither allowed group is refused.
    #[test]
    fn user_without_allowed_groups_is_refused() {
        assert_eq!(decide(&[]), Decision::NotAllowed);
        assert_eq!(decide(&[group(EVERYONE, ENABLED)]), Decision::NotAllowed);
    }

    // ADR 0021 §3: an enabled "Analyte users" membership allows.
    #[test]
    fn enabled_users_group_is_allowed() {
        let groups = [group(EVERYONE, ENABLED), group(USERS, ENABLED | 0x3)];
        assert_eq!(decide(&groups), Decision::Allowed);
    }

    // ADR 0021 §3: "Analyte users" counts only when enabled; deny-only or
    // disabled it never allows.
    #[test]
    fn users_group_deny_only_or_disabled_is_refused() {
        assert_eq!(
            decide(&[group(USERS, DENY_ONLY | ENABLED)]),
            Decision::NotAllowed
        );
        assert_eq!(decide(&[group(USERS, 0)]), Decision::NotAllowed);
    }

    // ADR 0021 §3 (owner, 2026-11-21): Administrators allow enabled or deny-only.
    #[test]
    fn administrators_enabled_or_deny_only_are_allowed() {
        assert_eq!(decide(&[group(ADMINISTRATORS, ENABLED)]), Decision::Allowed);
        assert_eq!(
            decide(&[group(ADMINISTRATORS, DENY_ONLY)]),
            Decision::Allowed
        );
        assert_eq!(decide(&[group(ADMINISTRATORS, 0)]), Decision::NotAllowed);
    }

    // ADR 0021 §3: without the local group only Administrators may sign in;
    // a same-named group elsewhere has another SID and never matches.
    #[test]
    fn missing_users_group_leaves_administrators_only() {
        let rule = Rule::new(None);
        assert_eq!(
            allowed(&[group(USERS, ENABLED)], &rule),
            Decision::NotAllowed
        );
        assert_eq!(
            allowed(&[group(ADMINISTRATORS, DENY_ONLY)], &rule),
            Decision::Allowed
        );
        assert_eq!(
            decide(&[group("S-1-5-21-9-9-9-1009", ENABLED)]),
            Decision::NotAllowed
        );
    }

    // ADR 0021 §3: the test-only rule leaves Administrators out.
    #[test]
    fn rule_without_administrators_refuses_them() {
        let rule = Rule::without_administrators(sid(USERS));
        assert_eq!(
            allowed(&[group(ADMINISTRATORS, ENABLED)], &rule),
            Decision::NotAllowed
        );
        assert_eq!(allowed(&[group(USERS, ENABLED)], &rule), Decision::Allowed);
    }

    // ADR 0021 §2: SIDs come from Windows in canonical text; anything else is refused.
    #[test]
    fn sid_text_must_be_canonical() {
        assert_eq!(Sid::new(USERS.into()).map(|s| s.0), Ok(USERS.to_owned()));
        for bad in [
            "",
            "S-1-",
            "S-1-5--1",
            "s-1-5-18",
            "S-1-5-x",
            "LAB-PC\\maria",
        ] {
            assert_eq!(Sid::new(bad.into()), Err(NotSid), "{bad}");
        }
        assert_eq!(Sid::new(format!("S-1-{}1", "1-".repeat(200))), Err(NotSid));
    }
}
