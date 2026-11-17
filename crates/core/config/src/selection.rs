//! Parses the legacy TOML layer names into one supported stack choice.
use crate::{Error, MachineDocument, invalid};

// Each choice and its legacy spellings are declared together. The macro makes
// the lookup exhaustive without a parallel enum or an index-coupled table.
macro_rules! stacks {
    ($( $(#[$doc:meta])* $choice:ident => $names:expr),+ $(,)?) => {
        /// One compatible protocol stack, including stream versus complete-file pickup.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum StackChoice { $( $(#[$doc])* $choice, )+ }
        impl StackChoice {
            pub(crate) fn layer_names(self) -> [&'static str; 4] {
                match self { $( Self::$choice => $names, )+ }
            }
        }
        const LAYOUTS: [(StackChoice, [&str; 4]); 8] = [
            $( (StackChoice::$choice, $names), )+
        ];
    };
}
stacks! {
    /// ASTM LIS01 and LIS02 over serial or TCP.
    AstmLis02 => ["astm", "astm_lis01", "astm_link", "astm_lis02"],
    /// HL7 v2 over MLLP and TCP.
    Hl7V2 => ["hl7", "mllp", "hl7_receiver", "hl7_v2"],
    /// ASTM LIS02 complete files.
    FileAstm => ["astm", "none", "file_pickup", "astm_lis02"],
    /// HL7 complete files.
    FileHl7 => ["hl7", "none", "file_pickup", "hl7_v2"],
    /// IHE LAW manager over MLLP and TCP.
    LawManager => ["law", "mllp", "law_manager", "hl7_v2"],
    /// POCT reviewer over TCP.
    Poct => ["poct", "poct", "poct", "poct"],
    /// VIDAS Literal stream.
    VidasLiteral => ["vidas", "vidas_literal", "vidas_literal", "vidas_literal"],
    /// Autolyser polling stream.
    Autolyser => ["autolyser", "autolyser", "autolyser", "autolyser"],
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
        let Some((choice, names)) = LAYOUTS.iter().find(|(_, names)| {
            names[1] == document.framing.kind
                && names[2] == document.dialog.kind
                && names[3] == document.syntax.kind
        }) else {
            return Err(Error::IncompatibleStack {
                path: path.into(),
                reason: "select a supported framing, dialog and syntax combination",
            });
        };
        let supported_transport = choice.supports_transport(t);
        if !supported_transport {
            return Err(Error::IncompatibleStack {
                path: path.into(),
                reason: "selected stack does not support this transport",
            });
        }
        let protocol = names[0];
        if document.protocol != protocol {
            return Err(invalid(
                path,
                "protocol",
                "selected protocol does not match syntax and dialog",
            ));
        }
        Ok(Self {
            choice: *choice,
            document,
        })
    }
}

fn wire_name<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    column: usize,
) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    if LAYOUTS
        .iter()
        .any(|(_, names)| names.get(column) == Some(&value.as_str()))
    {
        Ok(value)
    } else {
        let mut allowed = Vec::new();
        for (_, names) in LAYOUTS {
            if let Some(name) = names.get(column)
                && !allowed.contains(name)
            {
                allowed.push(*name);
            }
        }
        let value = quoted_layer(&value);
        Err(serde::de::Error::custom(format!(
            "unknown layer name {value}; expected one of {}",
            allowed.join(", ")
        )))
    }
}
pub(crate) fn protocol<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    wire_name(d, 0)
}
pub(crate) fn framing<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    wire_name(d, 1)
}
pub(crate) fn dialog<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    wire_name(d, 2)
}
pub(crate) fn syntax<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    wire_name(d, 3)
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
