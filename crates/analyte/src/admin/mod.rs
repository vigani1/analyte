//! Protected local administration; credentials remain owned by the host.
mod listener;
mod overview;
mod server;
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
