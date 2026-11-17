//! Validates ORU and OUL segment order before exposing a result message.
use crate::{Message, Msh, Version};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum Patient {
    Absent,
    Present,
    Visit,
    WithVisit,
    VisitExtra,
    WithExtendedVisit,
}
impl Patient {
    fn present(self) -> bool {
        matches!(
            self,
            Self::Present | Self::WithVisit | Self::WithExtendedVisit
        )
    }
    fn visit(self) -> bool {
        matches!(
            self,
            Self::Visit | Self::WithVisit | Self::VisitExtra | Self::WithExtendedVisit
        )
    }
    fn extra(self) -> bool {
        matches!(self, Self::VisitExtra | Self::WithExtendedVisit)
    }
}
#[derive(Clone, Copy)]
enum State {
    BeforeOrder(Patient),
    PendingControl(Patient),
    Specimen(Patient),
    OruOrder(Patient),
    OulOrder(Patient),
    OulControlledOrder(Patient),
}
impl State {
    fn patient(self) -> Patient {
        match self {
            Self::BeforeOrder(p)
            | Self::PendingControl(p)
            | Self::Specimen(p)
            | Self::OruOrder(p)
            | Self::OulOrder(p)
            | Self::OulControlledOrder(p) => p,
        }
    }
    fn order(self) -> bool {
        matches!(
            self,
            Self::OruOrder(_) | Self::OulOrder(_) | Self::OulControlledOrder(_)
        )
    }
    fn specimen(self) -> bool {
        matches!(
            self,
            Self::Specimen(_) | Self::OulOrder(_) | Self::OulControlledOrder(_)
        )
    }
    fn pending(self) -> bool {
        matches!(self, Self::PendingControl(_))
    }
    fn step(self, id: &[u8], previous: &[u8], index: usize, oul: bool) -> Option<Self> {
        let patient = self.patient();
        match id {
            b"PID"
                if !self.pending()
                    && (!patient.present() || (!oul && self.order()))
                    && (!oul || (!self.specimen() && !patient.visit())) =>
            {
                Some(Self::BeforeOrder(Patient::Present))
            }
            b"PV1"
                if !patient.visit()
                    && (oul || patient.present())
                    && !self.order()
                    && !self.specimen() =>
            {
                let p = if patient.present() {
                    Patient::WithVisit
                } else {
                    Patient::Visit
                };
                Some(if self.pending() {
                    Self::PendingControl(p)
                } else {
                    Self::BeforeOrder(p)
                })
            }
            b"PV2" if patient.visit() && !patient.extra() && !self.order() && !self.specimen() => {
                let p = if patient.present() {
                    Patient::WithExtendedVisit
                } else {
                    Patient::VisitExtra
                };
                Some(if self.pending() {
                    Self::PendingControl(p)
                } else {
                    Self::BeforeOrder(p)
                })
            }
            b"ORC" if oul && matches!(self, Self::OulOrder(_)) && previous == b"OBR" => {
                Some(Self::OulControlledOrder(patient))
            }
            b"ORC" if !oul && !self.pending() => Some(Self::PendingControl(patient)),
            b"OBR" if !oul || self.specimen() => Some(if oul {
                Self::OulOrder(patient)
            } else {
                Self::OruOrder(patient)
            }),
            b"SPM" if oul && (!self.specimen() || self.order()) => Some(Self::Specimen(patient)),
            b"SPM" if !oul && self.order() && !self.pending() => Some(self),
            b"SAC" if oul && self.specimen() && !self.order() => Some(self),
            b"OBX" if !self.pending() && (self.order() || (oul && self.specimen())) => Some(self),
            b"NTE"
                if !self.pending()
                    && (matches!(
                        previous,
                        b"PID" | b"OBR" | b"OBX" | b"NTE" | b"TCD" | b"SID"
                    ) || (oul && index == 1)) =>
            {
                Some(self)
            }
            b"TCD" if oul && self.order() && previous == b"OBX" => Some(self),
            b"SID" if oul && self.order() && matches!(previous, b"OBX" | b"TCD" | b"SID") => {
                Some(self)
            }
            _ => None,
        }
    }
}

/// A result message whose segment order satisfies the selected ORU/OUL grammar.
#[derive(Clone, Copy)]
enum Family {
    Oru,
    Oul,
}

/// Borrows a result message with validated segment order.
pub struct ResultMessage<'a> {
    family: Family,
    message: &'a Message,
    version: Version,
}
/// A result grammar refusal, with its one-based segment position.
#[derive(Debug)]
pub struct ResultGrammarError {
    /// One-based segment position of the refusal.
    pub record: usize,
    /// HL7 field position, or zero for a segment-order refusal.
    pub field: usize,
    /// Stable explanation for retaining the receipt.
    pub reason: &'static str,
}
impl<'a> ResultMessage<'a> {
    /// Checks HL7 2.3.1 §7.2.1 and 2.5.1 §7.3.1/§7.3.7 segment order.
    /// The caller supplies the already validated header.
    /// # Errors
    /// Rejects misplaced segments or an unfinished order group.
    pub fn new(message: &'a Message, msh: Msh<'a>) -> Result<Self, ResultGrammarError> {
        let parts: Vec<_> = msh.message_type().collect();
        let family = match parts.as_slice() {
            [b"ORU", b"R01"] | [b"ORU", b"R01", b"ORU_R01"] => Family::Oru,
            [b"OUL", b"R22"] | [b"OUL", b"R22", b"OUL_R22"] if msh.version().oul_reception() => {
                Family::Oul
            }
            _ => {
                return Err(ResultGrammarError {
                    record: 1,
                    field: 9,
                    reason: "only ORU R01 and edition 2.5.1 OUL R22 are supported",
                });
            }
        };
        let oul = matches!(family, Family::Oul);
        let mut state = State::BeforeOrder(Patient::Absent);
        let mut previous = b"MSH".as_slice();
        for (index, segment) in message.segments().enumerate().skip(1) {
            state = state.step(segment.id(), previous, index, oul).ok_or(ResultGrammarError {
                record: index.saturating_add(1),
                    field: 0,
                reason: "segment violates selected HL7 result grammar; retain receipt for review",
            })?;
            previous = segment.id();
        }
        if state.pending() || !state.order() {
            return Err(ResultGrammarError {
                record: message.segments().count(),
                field: 0,
                reason: "incomplete HL7 result group; retain receipt for review",
            });
        }
        Ok(Self {
            message,
            version: msh.version(),
            family,
        })
    }
    /// Reports whether this is the selected OUL specimen grammar.
    #[must_use]
    pub fn is_oul(&self) -> bool {
        matches!(self.family, Family::Oul)
    }
    /// Returns the original parsed message without rewriting fields.
    #[must_use]
    pub fn message(&self) -> &'a Message {
        self.message
    }
    /// Returns the edition validated by the header owner.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }
}
