//! Serves protected local administration pages and actions with host-owned credentials.
mod listener;
mod machines;
mod overview;
mod pages;
mod parked;
mod removed;
mod server;
mod sessions;
pub(crate) mod setup;
mod status;
mod token;
mod view;

pub(crate) use removed::{Draining, Kept};
pub(crate) use server::{Server, prepare};

pub use token::{Token, TokenError, rotate_token};

mod error;
pub use error::{CommandError, Error};

/// A temporary data directory with the owner-only mode the host requires on Unix
/// (the default temporary directory may allow group or other access).
#[cfg(test)]
pub(crate) fn private_tempdir() -> std::io::Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}
