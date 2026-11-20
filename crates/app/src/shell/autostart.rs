//! Starts the app when this Windows user signs in: one value named "Analyte" in the user's own Run key, with the app's quoted path and the tray-only argument.
//!
//! The installer writes the same value (its "Start Analyte when I sign in"
//! choice) and removes it on uninstall. Task Manager's Startup tab may turn
//! the entry off in `StartupApproved\Run`; the app reads that, and clears it
//! when the user turns starting back on from the tray.
use std::io;
use std::path::Path;
use windows_registry::CURRENT_USER;

/// The value's name, shared with `packaging/windows/analyte.iss`.
pub(super) const VALUE: &str = "Analyte";

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// What Task Manager writes for an entry it allows: an even first byte.
const ALLOWED: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The command Windows runs at sign-in: the quoted path, so a path with
/// spaces is never split, and the tray-only argument.
fn command(exe: &Path) -> String {
    format!("\"{}\" {}", exe.display(), super::TRAY_ONLY)
}

/// Whether Task Manager's value allows the entry: absent, or an even first byte.
fn allowed(approval: Option<&[u8]>) -> bool {
    approval.is_none_or(|bytes| bytes.first().is_some_and(|first| first % 2 == 0))
}

/// `ERROR_FILE_NOT_FOUND` as the registry functions report it (0x80070002).
const NOT_FOUND: i32 = i32::from_be_bytes([0x80, 0x07, 0x00, 0x02]);

/// A missing key or value reads as absent.
fn absent_is_none<T>(result: windows_registry::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code().0 == NOT_FOUND => Ok(None),
        Err(error) => Err(io::Error::from(error)),
    }
}

/// Whether the app starts at sign-in for this user.
///
/// # Errors
///
/// When the registry cannot be read.
pub(super) fn is_on() -> io::Result<bool> {
    let registered = absent_is_none(CURRENT_USER.open(RUN).and_then(|key| key.get_string(VALUE)))?;
    if registered.is_none() {
        return Ok(false);
    }
    let approval = absent_is_none(
        CURRENT_USER
            .open(APPROVED)
            .and_then(|key| key.get_value(VALUE)),
    )?;
    Ok(allowed(approval.as_deref()))
}

/// Turns starting at sign-in on or off for this user.
///
/// # Errors
///
/// When the registry cannot be written, or the app's path is unknown.
pub(super) fn set(on: bool) -> io::Result<()> {
    if on {
        let exe = std::env::current_exe()?;
        CURRENT_USER
            .create(RUN)
            .and_then(|key| key.set_string(VALUE, command(&exe)))
            .map_err(io::Error::from)?;
        // Only an entry Task Manager turned off has a value to clear.
        if let Some(key) = absent_is_none(CURRENT_USER.options().read().write().open(APPROVED))?
            && absent_is_none(key.get_value(VALUE))?.is_some()
        {
            key.set_bytes(VALUE, windows_registry::Type::Bytes, &ALLOWED)
                .map_err(io::Error::from)?;
        }
        return Ok(());
    }
    if let Some(key) = absent_is_none(CURRENT_USER.options().read().write().open(RUN))? {
        absent_is_none(key.remove_value(VALUE))?;
    }
    if let Some(key) = absent_is_none(CURRENT_USER.options().read().write().open(APPROVED))? {
        absent_is_none(key.remove_value(VALUE))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0019 (start at sign-in): the path is quoted, so "Program Files" is
    // never split, and the app starts in the tray only.
    #[test]
    fn the_command_quotes_the_path_and_starts_in_the_tray() {
        let exe = Path::new(r"C:\Program Files\Analyte\analyte-app.exe");
        assert_eq!(
            command(exe),
            r#""C:\Program Files\Analyte\analyte-app.exe" --tray"#
        );
    }

    // Task Manager's Startup tab: an even first byte allows the entry, an odd
    // one turns it off; no value means allowed.
    #[test]
    fn task_manager_approval_is_read() {
        assert!(allowed(None));
        assert!(allowed(Some(&ALLOWED)));
        assert!(allowed(Some(&[6, 0, 0, 0])));
        assert!(!allowed(Some(&[3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8])));
        assert!(!allowed(Some(&[])));
    }
}
