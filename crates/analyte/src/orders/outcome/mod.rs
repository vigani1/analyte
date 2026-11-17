//! Defines stable stored order and transfer outcome codes independently of variant names.
//!
//! Each protocol keeps its codes in its own file with one shape: a `<Key>End`
//! enum of fixed codes and, where the protocol stores diagnostic text, a
//! `<Key>Detail` enum. Codes the host writes for any protocol live in `host`.
//! This front file only dispatches.
mod astm;
mod autolyser;
mod hl7;
mod host;
mod law;
#[cfg(test)]
mod tests;
mod vidas;
pub(crate) use astm::AstmEnd;
pub(crate) use autolyser::{AutolyserEnd, Interruption};
pub(crate) use hl7::{Hl7Detail, Hl7End};
pub(crate) use host::HostEnd;
pub(crate) use law::{LawDetail, LawEnd};
pub(crate) use vidas::VidasEnd;

/// One fixed stored code of any protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Code {
    Host(HostEnd),
    Astm(AstmEnd),
    Autolyser(AutolyserEnd),
    Hl7(Hl7End),
    Law(LawEnd),
    Vidas(VidasEnd),
}
impl Code {
    /// Returns the explicit historical spelling; never derive evidence from Debug.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Host(code) => code.as_str(),
            Self::Astm(code) => code.as_str(),
            Self::Autolyser(code) => code.as_str(),
            Self::Hl7(code) => code.as_str(),
            Self::Law(code) => code.as_str(),
            Self::Vidas(code) => code.as_str(),
        }
    }
}

/// A closed transfer outcome; diagnostic text is bounded before durable storage.
pub(crate) enum Transfer {
    Code(Code),
    AutolyserInsertion(analyte_dialog::autolyser::Reception),
    AutolyserIncomplete(Interruption),
    AutolyserParked(analyte_dialog::autolyser::Error),
}
impl From<Code> for Transfer {
    fn from(code: Code) -> Self {
        Self::Code(code)
    }
}
/// Lets each protocol's fixed code stand for a [`Code`] and a [`Transfer`].
macro_rules! protocol_code {
    ($($variant:ident($code:ty)),+ $(,)?) => {$(
        impl From<$code> for Code {
            fn from(code: $code) -> Self {
                Self::$variant(code)
            }
        }
        impl From<$code> for Transfer {
            fn from(code: $code) -> Self {
                Self::Code(Code::$variant(code))
            }
        }
    )+};
}
protocol_code!(
    Host(HostEnd),
    Astm(AstmEnd),
    Autolyser(AutolyserEnd),
    Hl7(Hl7End),
    Law(LawEnd),
    Vidas(VidasEnd),
);
impl std::fmt::Display for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code(code) => f.write_str(code.as_str()),
            Self::AutolyserInsertion(reception) => f.write_str(&autolyser::insertion(*reception)),
            Self::AutolyserIncomplete(interruption) => f.write_str(&interruption.incomplete()),
            Self::AutolyserParked(error) => f.write_str(&autolyser::parked(error)),
        }
    }
}
