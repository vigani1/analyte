//! Writes stable acknowledgment evidence independently of Rust variant names.
use super::{AckCode, Condition};
use std::fmt;

impl fmt::Display for AckCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Aa => "Aa",
            Self::Ae => "Ae",
            Self::Ar => "Ar",
            Self::Ca => "Ca",
            Self::Ce => "Ce",
            Self::Cr => "Cr",
        })
    }
}

#[test]
fn ackcode_spellings_are_compatible() {
    for (value, expected) in [
        (AckCode::Aa, "Aa"),
        (AckCode::Ae, "Ae"),
        (AckCode::Ar, "Ar"),
        (AckCode::Ca, "Ca"),
        (AckCode::Ce, "Ce"),
        (AckCode::Cr, "Cr"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Always => "Always",
            Self::Never => "Never",
            Self::Error => "Error",
            Self::Success => "Success",
        })
    }
}

#[test]
fn condition_spellings_are_compatible() {
    for (value, expected) in [
        (Condition::Always, "Always"),
        (Condition::Never, "Never"),
        (Condition::Error, "Error"),
        (Condition::Success, "Success"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

/// Observed wire compatibility facts, independent of configuration declarations.
#[derive(Debug, PartialEq, Eq)]
pub struct WireCompatibility {
    /// At least one CR/LF pair appears in the retained message.
    pub crlf: bool,
    /// At least one LF appears in the retained message.
    pub lf: bool,
    /// MSH-18 explicitly declares the compatibility spelling UNICODE UTF-8.
    pub legacy_utf8: bool,
    /// A qualified category was read from MSH-16.
    pub msh16_message_category: Option<super::MessageCategory>,
    /// The character set used when MSH-18 is empty yet non-ASCII bytes were decoded
    /// with the connection's (machine definition's) set: a deliberate tolerance to note.
    pub charset_from_definition: Option<crate::Charset>,
    /// LAW MSH-7 as sent when it had no offset and was read in the site's time
    /// zone: a deliberate tolerance to note.
    pub header_time_without_zone: Option<Vec<u8>>,
}
/// Reads original boundaries and MSH-18 without normalizing the message.
#[must_use]
pub fn wire_compatibility(message: &crate::Message) -> WireCompatibility {
    let declared = message
        .segments()
        .next()
        .and_then(|msh| msh.field(18))
        .map_or(b"".as_slice(), crate::Field::raw);
    WireCompatibility {
        crlf: message.raw().windows(2).any(|bytes| bytes == b"\r\n"),
        lf: message.raw().contains(&b'\n'),
        msh16_message_category: super::Msh::new(message)
            .ok()
            .and_then(super::Msh::message_category),
        legacy_utf8: declared == b"UNICODE UTF-8",
        charset_from_definition: (declared.is_empty()
            && !message.raw().is_ascii()
            && message.charset() != crate::Charset::Ascii)
            .then(|| message.charset()),
        header_time_without_zone: crate::law::site_zone_header_time(message).map(<[u8]>::to_vec),
    }
}
// HL7 §2.4 boundaries; HL7 2.3.1 §2.24.1.18
#[test]
fn wire_evidence_keeps_original_bytes() -> Result<(), crate::Error> {
    for (ending, crlf, lf) in [
        ("\r", false, false),
        ("\r\n", true, true),
        ("\n", false, true),
    ] {
        let raw =
            format!("MSH|^~\\&|A|B|C|D|20261115||ORU^R01|id|P|2.3.1||||||UNICODE UTF-8{ending}");
        let message = crate::Message::parse(
            raw.as_bytes(),
            crate::Options {
                terminator: match ending {
                    "\r" => crate::Terminator::Cr,
                    "\r\n" => crate::Terminator::CrLf,
                    "\n" => crate::Terminator::Lf,
                    _ => unreachable!("fixed test cases"),
                },
                charset: crate::Charset::Utf8,
                allow_utf8_in_v231: true,
                ..crate::Options::default()
            },
        )?;
        assert_eq!(
            wire_compatibility(&message),
            WireCompatibility {
                crlf,
                lf,
                legacy_utf8: true,
                msh16_message_category: None,
                charset_from_definition: None,
                header_time_without_zone: None,
            }
        );
        assert_eq!(message.raw(), raw.as_bytes());
    }
    Ok(())
}
