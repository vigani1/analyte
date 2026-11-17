//! Answers protocol capability questions from the validated stack.
use super::{AstmOrders, Hl7Orders, LawStack, RecordTransport, Stack};

impl Stack {
    /// ASTM settings when this stack supports ASTM; otherwise absent.
    #[must_use]
    pub fn astm_orders(&self) -> Option<&AstmOrders> {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => s.orders(),
            Stack::Astm(RecordTransport::Folder(s)) => s.orders(),
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }
    /// HL7 settings when this stack supports HL7; otherwise absent.
    #[must_use]
    pub fn hl7_orders(&self) -> Option<&Hl7Orders> {
        match self {
            Stack::Hl7(RecordTransport::Stream(s)) => s.orders(),
            Stack::Hl7(RecordTransport::Folder(s)) => s.orders(),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Astm(RecordTransport::Folder(_)) => None,
        }
    }
    /// LAW settings when this stack supports LAW; otherwise absent.
    #[must_use]
    pub fn law(&self) -> Option<&LawStack> {
        match self {
            Stack::Law(s) => Some(s),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Astm(RecordTransport::Folder(_))
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }
    /// Whether this stack enables host-initiated work-list delivery.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => s.send_ahead(),
            Stack::Astm(RecordTransport::Folder(s)) => s.send_ahead(),
            Stack::Hl7(RecordTransport::Stream(s)) => s.send_ahead(),
            Stack::Hl7(RecordTransport::Folder(s)) => s.send_ahead(),
            Stack::Law(s) => s.send_ahead(),
            Stack::Vidas(s) => s.send_ahead(),
            Stack::Autolyser(s) => s.send_ahead(),
            Stack::Poct(s) => s.send_ahead(),
        }
    }
}

impl Stack {
    /// Returns the stable receipt driver namespace.
    #[must_use]
    pub fn driver_namespace(&self) -> &'static str {
        match self {
            Stack::Astm(RecordTransport::Stream(_)) | Stack::Astm(RecordTransport::Folder(_)) => {
                "analyte-astm"
            }
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Folder(_))
            | Stack::Law(_) => "analyte-hl7",
            Stack::Poct(_) => "analyte-poct",
            Stack::Vidas(_) => "analyte-vidas",
            Stack::Autolyser(_) => "analyte-autolyser",
        }
    }
}

impl crate::StackChoice {
    /// Returns whether this stack uses ASTM records.
    #[must_use]
    pub const fn has_astm_records(self) -> bool {
        match self {
            Self::AstmLis02 | Self::FileAstm => true,
            Self::Hl7V2
            | Self::FileHl7
            | Self::LawManager
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack uses HL7 segments.
    #[must_use]
    pub const fn has_hl7_records(self) -> bool {
        match self {
            Self::Hl7V2 | Self::FileHl7 | Self::LawManager => true,
            Self::AstmLis02
            | Self::FileAstm
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack requires field mappings.
    #[must_use]
    pub const fn has_field_mappings(self) -> bool {
        match self {
            Self::AstmLis02 | Self::Hl7V2 | Self::FileAstm | Self::FileHl7 | Self::LawManager => {
                true
            }
            Self::Poct | Self::VidasLiteral | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack requires a LAW section.
    #[must_use]
    pub const fn allows_law_section(self) -> bool {
        match self {
            Self::LawManager => true,
            Self::AstmLis02
            | Self::Hl7V2
            | Self::FileAstm
            | Self::FileHl7
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack uses the HL7 stream acknowledgment policy.
    #[must_use]
    pub const fn has_hl7_acknowledgment(self) -> bool {
        match self {
            Self::Hl7V2 | Self::LawManager => true,
            Self::AstmLis02
            | Self::FileAstm
            | Self::FileHl7
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack uses the LIS01 link.
    #[must_use]
    pub const fn has_astm_link(self) -> bool {
        match self {
            Self::AstmLis02 => true,
            Self::Hl7V2
            | Self::FileAstm
            | Self::FileHl7
            | Self::LawManager
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    /// Returns whether this stack uses VIDAS assay codes.
    #[must_use]
    pub const fn has_vidas_assay_codes(self) -> bool {
        match self {
            Self::VidasLiteral => true,
            Self::AstmLis02
            | Self::Hl7V2
            | Self::FileAstm
            | Self::FileHl7
            | Self::LawManager
            | Self::Poct
            | Self::Autolyser => false,
        }
    }
}

/// Grammar family used for field selectors and specimen code validation.
#[derive(Clone, Copy)]
pub(crate) enum RecordKind {
    Astm,
    Hl7,
    Vendor,
}
impl crate::StackChoice {
    pub(crate) const fn record_kind(self) -> RecordKind {
        match self {
            Self::AstmLis02 | Self::FileAstm => RecordKind::Astm,
            Self::Hl7V2 | Self::FileHl7 | Self::LawManager => RecordKind::Hl7,
            Self::Poct | Self::VidasLiteral | Self::Autolyser => RecordKind::Vendor,
        }
    }
    pub(crate) const fn has_hl7_departures(self) -> bool {
        match self {
            Self::Hl7V2 | Self::FileHl7 => true,
            Self::AstmLis02
            | Self::FileAstm
            | Self::LawManager
            | Self::Poct
            | Self::VidasLiteral
            | Self::Autolyser => false,
        }
    }
    pub(crate) const fn supports_transport(self, kind: crate::TransportKind) -> bool {
        use crate::TransportKind;
        match self {
            Self::AstmLis02 | Self::VidasLiteral | Self::Autolyser => {
                !matches!(kind, TransportKind::WatchedFolder)
            }
            Self::Hl7V2 | Self::LawManager | Self::Poct => {
                matches!(kind, TransportKind::TcpClient | TransportKind::TcpServer)
            }
            Self::FileAstm | Self::FileHl7 => matches!(kind, TransportKind::WatchedFolder),
        }
    }
}
