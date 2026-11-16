//! Serves protected local administration pages and actions with host-owned credentials.
mod actions;
mod listener;
mod overview;
mod pages;
mod retention;
mod server;
mod sessions;
mod token;

pub(crate) use server::prepare;

pub use token::{Token, TokenError, rotate_token};

/// Escapes text and attribute values through one HTML boundary.
fn escape(value: &str) -> String {
    value.chars().fold(String::new(), |mut out, character| {
        out.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                out.push(character);
                return out;
            }
        });
        out
    })
}

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
