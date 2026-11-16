//! Writes protected operator output to stdout.
use std::io::{self, Write};

/// Writes a line to the local operator console, which may contain protected evidence.
/// # Errors
/// Reports output errors, including a closed pipe.
pub fn line(text: &str) -> io::Result<()> {
    // Use a fallible writer so a closed pipe is an ordinary error, never a panic.
    let mut output = io::stdout().lock();
    writeln!(output, "{text}")
}
