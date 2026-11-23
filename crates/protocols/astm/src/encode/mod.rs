//! Encodes ASTM header, patient groups and termination records (LIS02-A2 sections 6, 7 and 12).
mod order;
pub use order::{OrderError, OrderRecord};

/// Explicit completion reason for an outbound order envelope.
#[derive(Clone, Copy, Debug)]
pub enum OrderTermination {
    /// Orders follow.
    Normal,
    /// The trusted cache contains no information.
    NoInformation,
    /// The query could not be answered reliably.
    QueryError,
}
/// Wraps bounded, already mapped orders; caller supplies the receiver and timestamp.
#[must_use]
pub fn encode_order_envelope(
    receiver: &str,
    timestamp: &str,
    orders: Vec<Vec<u8>>,
    termination: OrderTermination,
) -> Vec<Vec<u8>> {
    let receiver = escaped_receiver(receiver);
    let header = format!("H|\\^&|||Analyte|||||{receiver}||P|LIS2-A2|{timestamp}\r");
    envelope(header.into_bytes(), orders, code(termination))
}

fn code(termination: OrderTermination) -> &'static [u8] {
    match termination {
        OrderTermination::Normal => b"L|1|N\r",
        OrderTermination::NoInformation => b"L|1|I\r",
        OrderTermination::QueryError => b"L|1|Q\r",
    }
}

fn escaped_receiver(receiver: &str) -> String {
    receiver
        .replace('&', "&E&")
        .replace('|', "&F&")
        .replace('^', "&S&")
        .replace('\\', "&R&")
}

fn envelope(header: Vec<u8>, orders: Vec<Vec<u8>>, end: &[u8]) -> Vec<Vec<u8>> {
    let mut records = vec![header];
    for (index, order) in orders.into_iter().enumerate() {
        records.push(format!("P|{}\r", index.saturating_add(1)).into_bytes());
        records.push(order);
    }
    records.push(end.to_vec());
    records
}

/// Encodes a documented vendor envelope for query replies or batch downloads.
#[must_use]
pub fn encode_structured_order_envelope(
    profile: crate::OrderProfile,
    batch: bool,
    receiver: &str,
    orders: Vec<Vec<u8>>,
    termination: OrderTermination,
) -> Vec<Vec<u8>> {
    use crate::OrderProfile::{Cobas, Elecsys};
    let header = match profile {
        Elecsys => b"H|\\^&|||Analyte|||||||P||\r".to_vec(),
        Cobas => {
            let receiver = escaped_receiver(receiver);
            let cause = if batch { "BATCH" } else { "REPLY" };
            format!("H|\\^&|||Analyte^1|||||{receiver}|TSDWN^{cause}|P|1\r").into_bytes()
        }
    };
    let end = match (profile, termination) {
        (Elecsys, OrderTermination::Normal) => b"L|1|\r".as_slice(),
        (_, termination) => code(termination),
    };
    envelope(header, orders, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    // CLSI LIS02-A2 §6, §7, §12
    #[test]
    fn envelope_matches_legacy_bytes() {
        for (reason, code) in [
            (OrderTermination::Normal, b'N'),
            (OrderTermination::NoInformation, b'I'),
            (OrderTermination::QueryError, b'Q'),
        ] {
            for receiver in ["", "lab", "a&|^\\b"] {
                for orders in [vec![], vec![b"O|1|001a\r".to_vec(), b"O|1|002\r".to_vec()]] {
                    let escaped = receiver
                        .replace('&', "&E&")
                        .replace('|', "&F&")
                        .replace('^', "&S&")
                        .replace('\\', "&R&");
                    let mut old = vec![
                        format!("H|\\^&|||Analyte|||||{escaped}||P|LIS2-A2|20261117120000+0100\r")
                            .into_bytes(),
                    ];
                    for (index, order) in orders.iter().enumerate() {
                        old.push(format!("P|{}\r", index.saturating_add(1)).into_bytes());
                        old.push(order.clone());
                    }
                    old.push(vec![b'L', b'|', b'1', b'|', code, b'\r']);
                    assert_eq!(
                        encode_order_envelope(receiver, "20261117120000+0100", orders, reason),
                        old
                    );
                }
            }
        }
    }
}
