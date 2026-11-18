//! Writes explicit list, optional-value and quoted-text spellings and console lines for operator output.
use std::fmt::Display;
use std::io::{self, Write};
/// Keeps the historical list delimiters while formatting each value through Display.
pub fn list<T: Display>(values: &[T]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}
/// Keeps explicit absence distinct from a present Display value.
pub fn optional<T: Display>(value: Option<T>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| format!("Some({value})"))
}
/// Quotes source text explicitly; this is presentation, never a log payload.
#[must_use]
pub fn text(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        if character == '\'' {
            result.push(character);
        } else {
            result.extend(character.escape_debug());
        }
    }
    result.push('"');
    result
}
/// Preserves quoted text and distinguishes absence in optional operator fields.
pub fn optional_text(value: Option<&str>) -> String {
    optional(value.map(text))
}
#[test]
fn operator_container_spellings_are_compatible() {
    assert_eq!(list::<u64>(&[]), "[]");
    assert_eq!(list(&[1, 2]), "[1, 2]");
    assert_eq!(optional::<u64>(None), "None");
    assert_eq!(optional(Some(12)), "Some(12)");
    assert_eq!(text("a'\"?\n"), "\"a'\\\"?\\n\"");
    assert_eq!(optional_text(Some("x")), "Some(\"x\")");
}

/// Writes a line to the local operator console, which may contain protected evidence.
/// # Errors
/// Reports output errors, including a closed pipe.
pub fn line(text: &str) -> io::Result<()> {
    // Use a fallible writer so a closed pipe is an ordinary error, never a panic.
    let mut output = io::stdout().lock();
    writeln!(output, "{text}")
}
