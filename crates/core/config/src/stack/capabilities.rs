//! Answers protocol capability questions from the validated stack.
use super::*;

impl Stack {
    /// Returns the validated astm capability.
    #[must_use]
    pub fn astm_orders(&self) -> Option<&AstmOrders> {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => s.orders().as_ref(),
            Stack::Astm(RecordTransport::Folder(s)) => s.orders().as_ref(),
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Hl7(RecordTransport::Folder(_)) => None,
        }
    }
    /// Returns the validated hl7 capability.
    #[must_use]
    pub fn hl7_orders(&self) -> Option<&Hl7Orders> {
        match self {
            Stack::Hl7(RecordTransport::Stream(s)) => s.orders().as_ref(),
            Stack::Hl7(RecordTransport::Folder(s)) => s.orders().as_ref(),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_)
            | Stack::Astm(RecordTransport::Folder(_)) => None,
        }
    }
    /// Returns the validated law capability.
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
    /// Returns the validated send ahead capability.
    #[must_use]
    pub fn send_ahead(&self) -> bool {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => {
                s.orders().as_ref().is_some_and(AstmOrders::send_ahead)
            }
            Stack::Astm(RecordTransport::Folder(s)) => {
                s.orders().as_ref().is_some_and(AstmOrders::send_ahead)
            }
            Stack::Hl7(RecordTransport::Stream(s)) => {
                s.orders().as_ref().is_some_and(|o| o.send().is_some())
            }
            Stack::Hl7(RecordTransport::Folder(s)) => {
                s.orders().as_ref().is_some_and(|o| o.send().is_some())
            }
            Stack::Law(s) => s.orders().as_ref().is_some_and(LawOrders::send_ahead),
            Stack::Vidas(s) => s.orders().as_ref().is_some_and(VidasOrders::send_ahead),
            Stack::Autolyser(s) => s.orders().as_ref().is_some_and(AutolyserOrders::send_ahead),
            Stack::Poct(_) => false,
        }
    }
    /// Returns the selected stack's is law.
    #[must_use]
    pub fn atomic_batches(&self) -> bool {
        match self {
            Stack::Law(_) => true,
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Astm(RecordTransport::Folder(_))
            | Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Folder(_))
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_) => false,
        }
    }
    /// Returns the selected stack's astm syntax.
    #[must_use]
    pub fn astm_syntax(&self) -> Option<&AstmSyntax> {
        match self {
            Stack::Astm(RecordTransport::Stream(s)) => Some(s.syntax()),
            Stack::Astm(RecordTransport::Folder(s)) => Some(s.syntax()),
            Stack::Hl7(RecordTransport::Stream(_))
            | Stack::Hl7(RecordTransport::Folder(_))
            | Stack::Law(_)
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_) => None,
        }
    }
    /// Returns the selected stack's hl7 edition.
    #[must_use]
    pub fn hl7_edition(&self) -> Option<Edition> {
        match self {
            Stack::Hl7(RecordTransport::Stream(s)) => Some(s.syntax().edition()),
            Stack::Hl7(RecordTransport::Folder(s)) => Some(s.syntax().edition()),
            Stack::Law(_) => Some(Edition::V251),
            Stack::Astm(RecordTransport::Stream(_))
            | Stack::Astm(RecordTransport::Folder(_))
            | Stack::Poct(_)
            | Stack::Vidas(_)
            | Stack::Autolyser(_) => None,
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

    /// Returns whether each POCT reading has independent custody.
    #[must_use]
    pub fn independent_readings(&self) -> bool {
        match self {
            Self::Poct(_) => true,
            Self::Astm(_) | Self::Hl7(_) | Self::Law(_) | Self::Vidas(_) | Self::Autolyser(_) => {
                false
            }
        }
    }
}
