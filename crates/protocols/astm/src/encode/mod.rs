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
    mut orders: Vec<Vec<u8>>,
    termination: OrderTermination,
) -> Vec<Vec<u8>> {
    let termination = match termination {
        OrderTermination::Normal => b'N',
        OrderTermination::NoInformation => b'I',
        OrderTermination::QueryError => b'Q',
    };
    let receiver = receiver
        .replace('&', "&E&")
        .replace('|', "&F&")
        .replace('^', "&S&")
        .replace('\\', "&R&");
    let mut records =
        vec![format!("H|\\^&|||Analyte|||||{receiver}||P|LIS2-A2|{timestamp}\r").into_bytes()];
    for (index, order) in orders.drain(..).enumerate() {
        records.push(format!("P|{}\r", index.saturating_add(1)).into_bytes());
        records.push(order);
    }
    records.push(vec![b'L', b'|', b'1', b'|', termination, b'\r']);
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lis02_a2_sections_6_7_12_envelope_matches_legacy_bytes() {
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
