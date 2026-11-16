//! Decodes HL7 escapes and character sets while preserving unsupported formatting.
use super::*;
/// Checks bounded escape grammar without decoding or executing escapes.
pub(crate) fn validate_escapes(
    raw: &[u8],
    d: Delimiters,
    segment: usize,
    field: usize,
) -> Result<(), Error> {
    let mut parts = raw.split(|b| *b == d.escape);
    let mut offset = parts.next().map_or(0, <[u8]>::len);
    while let Some(code) = parts.next() {
        let valid = matches!(code, b"F" | b"S" | b"T" | b"R" | b"E" | b"H" | b"N")
            || (code == b"P" && d.truncation.is_some())
            || match code.first() {
                Some(b'X') => {
                    code.len() > 1
                        && code.len() % 2 == 1
                        && code
                            .get(1..)
                            .is_some_and(|s| s.iter().all(u8::is_ascii_hexdigit))
                }
                Some(b'Z') => code.len() > 1,
                Some(b'.') => formatting_command(code),
                Some(b'C') => {
                    code.len() == 5
                        && code
                            .get(1..)
                            .is_some_and(|s| s.iter().all(u8::is_ascii_hexdigit))
                }
                Some(b'M') => {
                    matches!(code.len(), 5 | 7)
                        && code
                            .get(1..)
                            .is_some_and(|s| s.iter().all(u8::is_ascii_hexdigit))
                }
                _ => false,
            };
        let tail = parts.next();
        if !valid || tail.is_none() {
            return Err(Error::Escape {
                segment,
                field,
                offset,
            });
        }
        offset = offset
            .saturating_add(code.len())
            .saturating_add(2)
            .saturating_add(tail.map_or(0, <[u8]>::len));
    }
    Ok(())
}
// HL7 2.5.1 section 2.7.6: recognize instructions without executing layout.
fn formatting_command(code: &[u8]) -> bool {
    if matches!(code, b".br" | b".fi" | b".nf" | b".ce" | b".sp") {
        return true;
    }
    let Some(command) = code.get(..3) else {
        return false;
    };
    let argument = code.get(3..).unwrap_or_default();
    let argument = argument.strip_prefix(b" ").unwrap_or(argument);
    let digits = match command {
        b".in" | b".ti" => argument
            .strip_prefix(b"+")
            .or_else(|| argument.strip_prefix(b"-"))
            .unwrap_or(argument),
        b".sp" | b".sk" => argument,
        _ => return false,
    };
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) && digits.iter().any(|b| *b != b'0')
}

fn decode(raw: &[u8], charset: Charset) -> Result<String, Error> {
    match charset {
        Charset::Ascii => {
            if raw.is_ascii() {
                String::from_utf8(raw.to_vec()).map_err(|_| Error::CharacterEncoding)
            } else {
                Err(Error::CharacterEncoding)
            }
        }
        Charset::Latin1 => Ok(raw.iter().map(|b| char::from(*b)).collect()),
        Charset::Utf8 => String::from_utf8(raw.to_vec()).map_err(|_| Error::CharacterEncoding),
    }
}

/// Escapes literal data with the message's declared delimiters, without truncating.
/// # Errors
/// Refuses control bytes, invalid character encoding, or output exceeding `maximum`.
pub fn encode_text(raw: &[u8], message: &Message, maximum: usize) -> Result<Vec<u8>, Error> {
    if raw.len() > maximum {
        return Err(Error::Limit {
            kind: "text bytes",
            segment: 0,
            field: 0,
        });
    }
    decode(raw, message.charset)?;
    let d = message.delimiters;
    let mut output = Vec::new();
    for byte in raw {
        if byte.is_ascii_control() {
            return Err(Error::Encoding);
        }
        let code = if *byte == d.field {
            Some(b'F')
        } else if *byte == d.component {
            Some(b'S')
        } else if *byte == d.repetition {
            Some(b'R')
        } else if *byte == d.subcomponent {
            Some(b'T')
        } else if *byte == d.escape {
            Some(b'E')
        } else if Some(*byte) == d.truncation {
            Some(b'P')
        } else {
            None
        };
        let encoded = [d.escape, code.unwrap_or_default(), d.escape];
        let bytes = if code.is_some() {
            &encoded[..]
        } else {
            std::slice::from_ref(byte)
        };
        crate::wire::append(&mut output, bytes, maximum).map_err(|_| Error::Limit {
            kind: "text bytes",
            segment: 0,
            field: 0,
        })?;
    }
    Ok(output)
}
/// Decodes delimiter escapes only, retaining hex, local, formatting and repertoire escapes.
/// # Errors
/// Returns an encoding error rather than replacing an undecodable byte.
pub fn decode_text(raw: &[u8], message: &Message) -> Result<String, Error> {
    if raw.len() > message.raw.len() {
        return Err(Error::Limit {
            kind: "text bytes",
            segment: 0,
            field: 0,
        });
    }
    let d = message.delimiters;
    let mut output = Vec::with_capacity(raw.len());
    let mut parts = raw.split(|b| *b == d.escape);
    output.extend_from_slice(parts.next().unwrap_or_default());
    while let Some(code) = parts.next() {
        let tail = parts.next();
        if matches!(code.first(), Some(b'C' | b'M')) && tail.is_some() {
            return Err(Error::UnsupportedCharset);
        }
        let replacement = match code {
            b"F" => Some(d.field),
            b"S" => Some(d.component),
            b"R" => Some(d.repetition),
            b"T" => Some(d.subcomponent),
            b"E" => Some(d.escape),
            b"P" => d.truncation,
            _ => None,
        };
        if let (Some(byte), Some(_)) = (replacement, tail) {
            output.push(byte);
        } else {
            output.push(d.escape);
            output.extend_from_slice(code);
            if tail.is_some() {
                output.push(d.escape);
            }
        }
        output.extend_from_slice(tail.unwrap_or_default());
    }
    decode(&output, message.charset)
}

/// Escape content requiring an explicit text handling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEscape {
    /// Non-delimiter escape or an unclosed escape.
    Unsupported,
    /// Highlighting or layout instructions.
    Formatting,
}

/// Inspects escapes without decoding them; formatting takes precedence over
/// earlier unsupported escapes, preserving conservative text handling.
#[must_use]
pub fn inspect_text_escapes(
    raw: &str,
    escape: u8,
    truncation_declared: bool,
) -> Option<TextEscape> {
    let escape = char::from(escape);
    let mut parts = raw.split(escape);
    parts.next();
    let mut issue = None;
    while let Some(code) = parts.next() {
        if parts.next().is_none() {
            return Some(TextEscape::Unsupported);
        }
        match code {
            "H" | "N" => return Some(TextEscape::Formatting),
            _ if code.starts_with('.') => return Some(TextEscape::Formatting),
            "F" | "S" | "T" | "R" | "E" => {}
            "P" if truncation_declared => {}
            _ => issue = Some(TextEscape::Unsupported),
        }
    }
    issue
}

// Declared encodings are checked before any decoding; never guess from bytes.
pub(super) fn declared_charset(
    msh: Segment<'_>,
    version_id: &[u8],
    options: Options,
    law: bool,
) -> Result<Charset, Error> {
    let charset = match msh.field(18).map(|f| f.raw).unwrap_or_default() {
        b"" | b"ASCII" => Charset::Ascii,
        b"8859/1" => Charset::Latin1,
        b"UNICODE UTF-8"
            if crate::Version::standard_utf8(version_id)
                || (law && version_id.starts_with(b"2.5"))
                || (version_id == b"2.3.1" && options.allow_utf8_in_v231) =>
        {
            Charset::Utf8
        }
        _ => return Err(Error::UnsupportedCharset),
    };
    if charset != options.charset {
        return Err(Error::CharsetMismatch);
    }
    if !law && msh.field(20).is_some_and(|f| !f.raw.is_empty()) {
        return Err(Error::UnsupportedCharset);
    }
    Ok(charset)
}
