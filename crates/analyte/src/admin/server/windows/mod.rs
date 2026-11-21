//! Windows sign-in for the Analyte app (ADR 0021): who may sign in, the pipe's two lines, the one-time codes, the account a session is signed in as and whether sign-in is on; on Windows also the pipe the engine serves them on.
pub(super) mod codes;
pub(super) mod line;
#[cfg(windows)]
mod pipe;
pub(super) mod rule;
use analyte_model::Identifier;
#[cfg(windows)]
pub(super) use pipe::{Start, start};
use tokio::sync::watch;

/// The longest account name kept, in bytes (ADR 0021 §5: it must fit the
/// audit actor; a longer one is refused, never cut).
const NAME_LIMIT: usize = 256;
/// Canonical SID text is far shorter (ADR 0017's read bound).
const SID_LIMIT: usize = 256;
/// How a Windows session's audit actor begins (ADR 0021 §5); the command
/// line refuses actors that begin so.
pub(in crate::admin) const ACTOR_PREFIX: &str = "windows:";

/// A verified account's name as Windows gave it, `DOMAIN\name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct AccountName(String);

/// Why text is not an account name the engine keeps.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a DOMAIN\\name account name of at most 256 bytes")]
pub(in crate::admin) struct NotName;

impl AccountName {
    /// Accepts `DOMAIN\name` with both parts non-empty, one backslash, no `:`
    /// (Windows names cannot hold one, so the audit spelling reads back) and no
    /// control characters, in at most [`NAME_LIMIT`] bytes.
    /// # Errors
    /// [`NotName`] for anything else.
    pub(in crate::admin) fn new(text: String) -> Result<Self, NotName> {
        let parts = text.split_once('\\');
        let shaped = parts.is_some_and(|(domain, name)| {
            !domain.is_empty() && !name.is_empty() && !name.contains('\\')
        });
        let clean = !text.chars().any(|c| c == ':' || c.is_control());
        if shaped && clean && text.len() <= NAME_LIMIT {
            Ok(Self(text))
        } else {
            Err(NotName)
        }
    }

    /// `DOMAIN\name`.
    pub(in crate::admin) fn as_str(&self) -> &str {
        &self.0
    }
}

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

/// The Windows account a session is signed in as: verified by Windows, with
/// the audit actor that names it made once, when the account is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct WindowsAccount {
    sid: Sid,
    name: Option<AccountName>,
    actor: Identifier,
}

/// Why a Windows account cannot be recorded as an audit actor.
#[derive(Debug, thiserror::Error)]
#[error("the account's audit name is longer than an actor may be")]
pub(in crate::admin) struct ActorTooLong(#[source] analyte_model::TextError);

impl WindowsAccount {
    /// The account `sid`, named `name` when Windows found it. Its actor is
    /// `windows:<SID>:<DOMAIN>\<name>`, or `windows:<SID>` without a name.
    /// # Errors
    /// [`ActorTooLong`] when that spelling is longer than an actor may be:
    /// it is refused, never cut.
    pub(in crate::admin) fn new(sid: Sid, name: Option<AccountName>) -> Result<Self, ActorTooLong> {
        let spelled = match &name {
            Some(name) => format!("{ACTOR_PREFIX}{}:{}", sid.as_str(), name.as_str()),
            None => format!("{ACTOR_PREFIX}{}", sid.as_str()),
        };
        let actor = Identifier::new(&spelled).map_err(ActorTooLong)?;
        Ok(Self { sid, name, actor })
    }

    /// How pages name the account: `DOMAIN\name`, or the SID without a name.
    pub(in crate::admin) fn shown(&self) -> &str {
        self.name
            .as_ref()
            .map_or_else(|| self.sid.as_str(), AccountName::as_str)
    }

    /// The audit actor that records what this account does.
    pub(in crate::admin) fn actor(&self) -> &Identifier {
        &self.actor
    }
}

/// Whether Windows sign-in works in this run, as the pages say it.
/// The local group whose enabled members may sign in (ADR 0021 §3).
#[cfg(windows)]
pub(in crate::admin) const GROUP: &str = "Analyte users";

/// What looking up the local [`GROUP`] found.
#[cfg(windows)]
pub(in crate::admin) enum Found {
    /// The group exists; its SID.
    Group(Sid),
    /// An account has the name but is not a local group.
    NotAGroup,
    /// No account has the name.
    Missing,
    /// `COMPUTERNAME` is not set, so the local name cannot be qualified.
    NoComputerName,
    /// Windows could not look the name up.
    Failed(crate::runtime::service::caller::Os),
}

/// Looks up `<COMPUTERNAME>\Analyte users`, qualified so a same-named domain
/// group never matches; read-only, through the existing safe account lookup.
#[cfg(windows)]
pub(in crate::admin) fn users_group() -> Found {
    let Some(computer) = std::env::var_os("COMPUTERNAME") else {
        return Found::NoComputerName;
    };
    let qualified = format!("{}\\{GROUP}", computer.to_string_lossy());
    match crate::runtime::service::caller::account_sid(&qualified) {
        Ok(Some((sid, kind))) if kind == windows_sys::Win32::Security::SidTypeAlias => {
            Sid::new(sid).map_or(Found::NotAGroup, Found::Group)
        }
        Ok(Some(_)) => Found::NotAGroup,
        Ok(None) => Found::Missing,
        Err(error) => Found::Failed(error),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Status {
    /// Members of "Analyte users" and Administrators may sign in.
    On,
    /// The "Analyte users" group is missing, so only Administrators may.
    AdministratorsOnly,
    /// Off for this run; the words say why, without any account name.
    Off(String),
}

impl Status {
    /// The Settings page's words for it.
    pub(in crate::admin) fn line(&self) -> String {
        match self {
            Self::On => "On".into(),
            Self::AdministratorsOnly => concat!(
                "Off for other users because the Analyte users group is missing, ",
                "so only administrators can sign in with Windows"
            )
            .into(),
            Self::Off(why) => format!("Off because {why}"),
        }
    }
}

/// Windows sign-in as the pages see it: the codes waiting for
/// `GET /login/windows` and whether sign-in is on.
pub(in crate::admin) struct SignIn {
    /// The codes the pipe handed out and not yet taken back.
    pub(in crate::admin) codes: codes::Codes,
    /// None on a system without Windows sign-in (ADR 0021 §9).
    status: watch::Sender<Option<Status>>,
}

impl SignIn {
    /// Sign-in with `status`, or none on a system without it, and no codes.
    pub(in crate::admin) fn new(status: Option<Status>) -> Self {
        Self {
            codes: codes::Codes::default(),
            status: watch::channel(status).0,
        }
    }

    /// Whether Windows sign-in is on now; none on a system without it.
    pub(in crate::admin) fn status(&self) -> Option<Status> {
        self.status.borrow().clone()
    }

    /// Sets whether sign-in is on, as when the pipe stops serving.
    pub(in crate::admin) fn set(&self, status: Status) {
        self.status.send_replace(Some(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "S-1-5-21-1-2-3-1001";

    // ADR 0021 §5: names are DOMAIN\name, bounded, without ':' or controls.
    #[test]
    fn account_names_are_domain_and_name() {
        let name = AccountName::new("LAB-PC\\maria".into());
        assert_eq!(name.as_ref().map(AccountName::as_str), Ok("LAB-PC\\maria"));
        for bad in [
            "maria",
            "\\maria",
            "LAB-PC\\",
            "LAB-PC\\ma\\ria",
            "LAB-PC\\ma:ria",
            "LAB-PC\\ma\nria",
        ] {
            assert_eq!(AccountName::new(bad.into()), Err(NotName), "{bad}");
        }
        let long = format!("LAB-PC\\{}", "m".repeat(NAME_LIMIT));
        assert_eq!(AccountName::new(long), Err(NotName));
    }

    // ADR 0021 §2: SIDs come from Windows in canonical text; anything else is refused.
    #[test]
    fn sid_text_must_be_canonical() {
        assert_eq!(Sid::new(USER.into()).map(|s| s.0), Ok(USER.to_owned()));
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

    // ADR 0021 §5: the actor is windows:<SID>:<DOMAIN>\<name>, or
    // windows:<SID> without a name; pages show the name, or the SID.
    #[test]
    fn account_actor_names_sid_and_account() -> Result<(), Box<dyn std::error::Error>> {
        let named = WindowsAccount::new(
            Sid::new(USER.into())?,
            Some(AccountName::new("LAB-PC\\maria".into())?),
        )?;
        assert_eq!(
            named.actor().as_str(),
            "windows:S-1-5-21-1-2-3-1001:LAB-PC\\maria"
        );
        assert_eq!(named.shown(), "LAB-PC\\maria");
        let bare = WindowsAccount::new(Sid::new(USER.into())?, None)?;
        assert_eq!(bare.actor().as_str(), "windows:S-1-5-21-1-2-3-1001");
        assert_eq!(bare.shown(), USER);
        Ok(())
    }

    // ADR 0021 §5: a spelling over the actor limit is refused, never cut.
    #[test]
    fn account_actor_over_the_limit_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let sid = Sid::new(format!("S-1-5-21-{}", "1".repeat(200)))?;
        let name = AccountName::new(format!("LAB-PC\\{}", "m".repeat(100)))?;
        assert!(WindowsAccount::new(sid.clone(), Some(name)).is_err());
        assert!(WindowsAccount::new(sid, None).is_ok());
        Ok(())
    }

    // ADR 0021 §1 and §3: Settings says whether sign-in is on, and why not.
    #[test]
    fn status_says_on_or_why_off() {
        assert_eq!(Status::On.line(), "On");
        assert!(
            Status::AdministratorsOnly
                .line()
                .contains("the Analyte users group is missing, so only administrators")
        );
        let taken = Status::Off("another program already holds the sign-in pipe".into());
        assert_eq!(
            taken.line(),
            "Off because another program already holds the sign-in pipe"
        );
        assert_eq!(SignIn::new(None).status(), None);
        let sign_in = SignIn::new(Some(Status::On));
        assert_eq!(sign_in.status(), Some(Status::On));
        sign_in.set(taken.clone());
        assert_eq!(sign_in.status(), Some(taken));
    }
}
