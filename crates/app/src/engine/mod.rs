//! Where the engine's admin pages are: the admin port, read from the settings file the installer writes beside the app, or the starter program's port.
//!
//! The engine's own program file sits in a folder only administrators and the
//! service may read, so the installer copies its `[admin] port` into
//! `analyte-app.toml` next to `analyte-app.exe`, in the program folder only
//! administrators may change. The address is always the loopback one the
//! engine binds; only the port can differ.
use std::path::{Path, PathBuf};

pub mod service;
pub mod status;

/// The settings file's name, beside the app.
pub const SETTINGS: &str = "analyte-app.toml";

/// The admin port of the starter program the installer writes
/// (`packaging/windows/starter/analyte.toml`), used when no settings file exists.
pub const DEFAULT_PORT: u16 = 8080;

/// The most the settings file may hold; it has one line that matters.
const SETTINGS_LIMIT: u64 = 4096;

/// The engine the app watches and opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    port: u16,
}

/// What the settings file holds.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// The engine's admin port, as its program file's `[admin] port`.
    admin_port: u16,
}

/// Why the settings file beside the app could not be used. The app then
/// shows this on its Engine off screen and contacts no address.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The file exists but could not be read.
    #[error("the app could not read {path}: {source}")]
    Read {
        /// The settings file.
        path: PathBuf,
        /// Why reading failed.
        source: std::io::Error,
    },
    /// The file holds more than the one setting it may.
    #[error("{path} is larger than {SETTINGS_LIMIT} bytes")]
    TooLarge {
        /// The settings file.
        path: PathBuf,
    },
    /// The file is not the settings the installer writes.
    #[error("{path} is not valid: {source}")]
    Invalid {
        /// The settings file.
        path: PathBuf,
        /// What is wrong in it.
        source: toml::de::Error,
    },
    /// The file names port 0.
    #[error("{path} gives admin port 0, which is not a port")]
    PortZero {
        /// The settings file.
        path: PathBuf,
    },
}

impl Engine {
    /// The engine at `port` on this computer, or none for port 0.
    #[must_use]
    pub const fn at(port: u16) -> Option<Self> {
        if port == 0 { None } else { Some(Self { port }) }
    }

    /// The engine named by `folder`'s settings file, or the starter program's
    /// port when the file does not exist.
    ///
    /// # Errors
    ///
    /// A settings file that exists but cannot be read, is too large or is not
    /// valid; the app never guesses another port then.
    pub fn from_settings(folder: &Path) -> Result<Self, SettingsError> {
        use std::io::Read as _;
        let path = folder.join(SETTINGS);
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { port: DEFAULT_PORT });
            }
            Err(source) => return Err(SettingsError::Read { path, source }),
        };
        let mut text = String::new();
        file.take(SETTINGS_LIMIT.saturating_add(1))
            .read_to_string(&mut text)
            .map_err(|source| SettingsError::Read {
                path: path.clone(),
                source,
            })?;
        if u64::try_from(text.len()).map_or(true, |length| length > SETTINGS_LIMIT) {
            return Err(SettingsError::TooLarge { path });
        }
        let settings: Settings =
            toml::from_str(&text).map_err(|source| SettingsError::Invalid {
                path: path.clone(),
                source,
            })?;
        Self::at(settings.admin_port).ok_or(SettingsError::PortZero { path })
    }

    /// The admin pages' origin, such as `http://127.0.0.1:8080`; the window
    /// shows only this origin and the app's own pages.
    #[must_use]
    pub const fn origin(&self) -> Origin {
        Origin::Engine(self.port)
    }

    /// The Overview, where the window opens.
    #[must_use]
    pub fn home(&self) -> String {
        format!("{}/", self.origin().text())
    }

    /// The public `GET /status` the tray reads.
    #[must_use]
    pub fn status(&self) -> String {
        format!("{}/status", self.origin().text())
    }
}

/// An origin the window may show: the engine's pages, or the app's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `http://127.0.0.1:<port>`, exactly as the engine's Host check wants it.
    Engine(u16),
    /// `http://analyte.localhost`, where `WebView2` serves the app's
    /// [`LOCAL_SCHEME`] (the Engine off screen and its files).
    Local,
}

/// The scheme the app registers for its own pages.
pub const LOCAL_SCHEME: &str = "analyte";

/// The host `WebView2` gives [`LOCAL_SCHEME`].
const LOCAL_HOST: &str = "analyte.localhost";

impl Origin {
    /// The origin as text, such as `http://127.0.0.1:8080`, with no path.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Engine(port) => format!("http://127.0.0.1:{port}"),
            Self::Local => format!("http://{LOCAL_HOST}"),
        }
    }

    /// Whether `url` is on this origin: the same scheme, host and port, with
    /// no user name or password.
    #[must_use]
    pub fn holds(self, url: &url::Url) -> bool {
        let (host, port) = match self {
            Self::Engine(port) => ("127.0.0.1", port),
            Self::Local => (LOCAL_HOST, 80),
        };
        url.scheme() == "http"
            && url.username().is_empty()
            && url.password().is_none()
            && url.host_str() == Some(host)
            && url.port_or_known_default() == Some(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder_with(text: &str) -> tempfile::TempDir {
        let folder = tempfile::tempdir().expect("a temporary folder");
        std::fs::write(folder.path().join(SETTINGS), text).expect("settings written");
        folder
    }

    // ADR 0019: the installer copies the program's admin port beside the app;
    // with no file, the app uses the starter program's port.
    #[test]
    fn settings_give_the_admin_port_or_the_starter_port() {
        let folder = folder_with("admin_port = 4100\n");
        let engine = Engine::from_settings(folder.path()).expect("valid settings");
        assert_eq!(engine.origin(), Origin::Engine(4100));
        assert_eq!(engine.home(), "http://127.0.0.1:4100/");
        assert_eq!(engine.status(), "http://127.0.0.1:4100/status");
        let empty = tempfile::tempdir().expect("a temporary folder");
        let starter = Engine::from_settings(empty.path()).expect("no file is the starter port");
        assert_eq!(starter.origin().text(), "http://127.0.0.1:8080");
    }

    // AGENTS.md (never guess): a settings file that exists but is wrong is
    // refused with why, never replaced by the starter port.
    #[test]
    fn settings_that_are_wrong_are_refused() {
        for text in [
            "admin_port = 0\n",
            "admin_port = 70000\n",
            "admin_port = \"8080\"\n",
            "port = 8080\n",
            "admin_port = 8080\nhost = \"example.org\"\n",
        ] {
            let folder = folder_with(text);
            assert!(Engine::from_settings(folder.path()).is_err(), "{text}");
        }
        let large = folder_with(&format!("# {}\nadmin_port = 8080\n", "x".repeat(5000)));
        assert!(matches!(
            Engine::from_settings(large.path()),
            Err(SettingsError::TooLarge { .. })
        ));
    }

    #[test]
    fn local_origin_is_the_webview2_scheme_host() {
        assert_eq!(Origin::Local.text(), "http://analyte.localhost");
    }
}
