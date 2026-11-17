//! Reads LAW custody identities and execution controls (TF-2b B.7 and C.6).
use crate::{Field, Message, Segment};
fn raw(segment: Segment<'_>, field: usize) -> &[u8] {
    segment.field(field).map_or(b"", Field::raw)
}
/// Immutable header inputs for host-owned custody identity and evidence.
pub struct CustodyHeader<'a>(Segment<'a>);
impl<'a> CustodyHeader<'a> {
    /// Reads the already parsed message header without inventing missing fields.
    #[must_use]
    pub fn new(message: &'a Message) -> Option<Self> {
        message
            .segments()
            .next()
            .filter(|s| s.id() == b"MSH")
            .map(Self)
    }
    /// Returns MSH-3 exactly as sent for custody scope.
    #[must_use]
    pub fn application(&self) -> &'a [u8] {
        raw(self.0, 3)
    }
    /// Returns MSH-4 exactly as sent for custody scope.
    #[must_use]
    pub fn facility(&self) -> &'a [u8] {
        raw(self.0, 4)
    }
    /// Returns the original message control identity.
    #[must_use]
    pub fn message_id(&self) -> &'a [u8] {
        raw(self.0, 10)
    }
    /// Returns the timestamp, retaining its precision and zone.
    #[must_use]
    pub fn timestamp(&self) -> &'a [u8] {
        raw(self.0, 7)
    }
    /// Recognizes production from PT.1 only.
    #[must_use]
    pub fn production(&self) -> bool {
        raw(self.0, 11)
            .split(|b| *b == self.0.delimiters.component)
            .next()
            == Some(b"P".as_slice())
    }
}
/// Custody-relevant source records; unrelated segments carry no work transition.
pub enum CustodyRecord<'a> {
    /// An OBR, including explicit null for unsolicited work.
    Work {
        /// Exact EI.1, or None for the LAW null marker.
        placer: Option<&'a [u8]>,
    },
    /// Original control retained as evidence beside its completion flag.
    Control {
        /// Original ORC, never reconstructed.
        segment: Segment<'a>,
        /// Exact ORC-5 CM indication.
        complete: bool,
    },
    /// A segment outside custody control.
    Other,
}
/// Walks source order/control records without interpreting clinical results.
pub fn custody_records(message: &Message) -> impl Iterator<Item = CustodyRecord<'_>> {
    message.segments().map(|segment| match segment.id() {
        b"OBR" => CustodyRecord::Work {
            placer: (raw(segment, 2) != b"\"\"").then(|| {
                raw(segment, 2)
                    .split(|b| *b == segment.delimiters.component)
                    .next()
                    .unwrap_or_default()
            }),
        },
        b"ORC" => CustodyRecord::Control {
            segment,
            complete: raw(segment, 5) == b"CM",
        },
        _ => CustodyRecord::Other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn law_tf2b_b7_c6_c8_custody_fields_preserve_original_spelling() -> Result<(), crate::Error> {
        for (enc, component) in [("^~\\&", '^'), ("$~\\&", '$')] {
            let bytes = format!(
                "MSH|{enc}|A|AF|M|MF|20260101120000+0000||OUL{component}R22{component}OUL_R22|m1|P{component}T|2.5.1\rOBR||awos-01{component}LAB\rORC|||||CM\rOBR||\"\"\r"
            );
            let message = Message::parse(bytes.as_bytes(), crate::Options::default())?;
            let header = CustodyHeader::new(&message).expect("synthetic header");
            assert_eq!(header.application(), b"A");
            assert_eq!(header.facility(), b"AF");
            assert_eq!(header.message_id(), b"m1");
            assert_eq!(header.timestamp(), b"20260101120000+0000");
            assert!(header.production());
            let mut records =
                custody_records(&message).filter(|r| !matches!(r, CustodyRecord::Other));
            assert!(matches!(
                records.next(),
                Some(CustodyRecord::Work {
                    placer: Some(b"awos-01")
                })
            ));
            assert!(matches!(
                records.next(),
                Some(CustodyRecord::Control { complete: true, .. })
            ));
            assert!(matches!(
                records.next(),
                Some(CustodyRecord::Work { placer: None })
            ));
        }
        Ok(())
    }
    #[test]
    fn law_tf2b_c8_status_rendering_matches_legacy_bytes() {
        use super::super::OrderStatus;
        for status in [
            OrderStatus::Scheduled,
            OrderStatus::InProgress,
            OrderStatus::Complete,
            OrderStatus::Cancelled,
        ] {
            let old = match status {
                OrderStatus::Scheduled => "SC",
                OrderStatus::InProgress => "IP",
                OrderStatus::Complete => "CM",
                OrderStatus::Cancelled => "CA",
            };
            assert_eq!(status.as_bytes().escape_ascii().to_string(), old);
        }
    }
}
