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
