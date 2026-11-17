//! Derives a supported stack from the protocol key and physical transport.
use crate::{Error, MachineDocument, invalid};

/// One compatible protocol stack, including stream versus complete-file pickup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackChoice {
    /// ASTM LIS01 and LIS02 over serial or TCP.
    AstmLis02,
    /// HL7 v2 over MLLP and TCP.
    Hl7V2,
    /// ASTM LIS02 complete files.
    FileAstm,
    /// HL7 complete files.
    FileHl7,
    /// IHE LAW manager over MLLP and TCP.
    LawManager,
    /// POCT reviewer over TCP.
    Poct,
    /// VIDAS Literal stream.
    VidasLiteral,
    /// Autolyser polling stream.
    Autolyser,
}

pub(crate) struct ParsedMachine<'a> {
    choice: StackChoice,
    document: &'a MachineDocument,
}
impl std::ops::Deref for ParsedMachine<'_> {
    type Target = MachineDocument;
    fn deref(&self) -> &Self::Target {
        self.document
    }
}
impl<'a> ParsedMachine<'a> {
    pub(crate) fn choice(&self) -> StackChoice {
        self.choice
    }

    pub(crate) fn parse(document: &'a MachineDocument, path: &str) -> Result<Self, Error> {
        let t = document.transport.kind;
        let choice = derive(&document.protocol, t, path)?;
        if !choice.supports_transport(t) {
            return Err(Error::IncompatibleStack {
                path: path.into(),
                reason: "selected stack does not support this transport",
            });
        }

        Ok(Self { choice, document })
    }
}

pub(crate) fn derive(
    protocol: &str,
    transport: crate::TransportKind,
    path: &str,
) -> Result<StackChoice, Error> {
    let choice = match (protocol, transport) {
        ("astm", crate::TransportKind::WatchedFolder) => StackChoice::FileAstm,
        ("hl7", crate::TransportKind::WatchedFolder) => StackChoice::FileHl7,
        ("astm", _) => StackChoice::AstmLis02,
        ("hl7", _) => StackChoice::Hl7V2,
        ("law", _) => StackChoice::LawManager,
        ("poct", _) => StackChoice::Poct,
        ("vidas", _) => StackChoice::VidasLiteral,
        ("autolyser", _) => StackChoice::Autolyser,
        _ => return Err(invalid(path, "protocol", "unknown protocol key")),
    };
    Ok(choice)
}
pub(crate) fn protocol<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(d)?;
    if matches!(
        value.as_str(),
        "astm" | "hl7" | "law" | "poct" | "vidas" | "autolyser"
    ) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(format!(
            "unknown protocol key {}",
            quoted_layer(&value)
        )))
    }
}

// Display preserves historical evidence names, independently of the TOML keys.
impl std::fmt::Display for StackChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AstmLis02 => "AstmLis02",
            Self::Hl7V2 => "Hl7V2",
            Self::FileAstm => "FileAstm",
            Self::FileHl7 => "FileHl7",
            Self::LawManager => "LawManager",
            Self::Poct => "Poct",
            Self::VidasLiteral => "VidasLiteral",
            Self::Autolyser => "Autolyser",
        })
    }
}

#[test]
fn stack_evidence_spellings_are_compatible() {
    for (value, expected) in [
        (StackChoice::AstmLis02, "AstmLis02"),
        (StackChoice::Hl7V2, "Hl7V2"),
        (StackChoice::FileAstm, "FileAstm"),
        (StackChoice::FileHl7, "FileHl7"),
        (StackChoice::LawManager, "LawManager"),
        (StackChoice::Poct, "Poct"),
        (StackChoice::VidasLiteral, "VidasLiteral"),
        (StackChoice::Autolyser, "Autolyser"),
    ] {
        assert_eq!(value.to_string(), expected);
    }
}

// Configuration refusals keep their historical quotation without relying on Debug.
fn quoted_layer(value: &str) -> String {
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
#[test]
fn layer_refusal_quotation_is_compatible() {
    assert_eq!(quoted_layer("a'\"?\n"), "\"a'\\\"?\\n\"");
    assert_eq!(quoted_layer(""), "\"\"");
}
