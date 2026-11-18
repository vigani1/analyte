//! Checks retained receipt decoding against live framing boundaries (ADR 0003).
use analyte_framing::{astm, hl7};

// CLSI LIS01-A2 §6.3
#[test]
fn retained_frames_share_live_checks_and_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let limit = astm::Limit::new(12)?;
    let text = b"R|synthetic|value\r";
    let frames: Vec<_> = astm::encode(text, astm::FrameNumber::FIRST, limit)?.collect();
    let raw: Vec<_> = frames.iter().flatten().copied().collect();
    assert_eq!(
        astm::decode(
            &raw,
            limit,
            astm::HexPolicy::Strict,
            astm::RecordBoundaries::Separate,
            text.len()
        )?,
        text
    );
    assert!(matches!(
        astm::decode(
            &raw,
            limit,
            astm::HexPolicy::Strict,
            astm::RecordBoundaries::Separate,
            text.len().saturating_sub(1)
        ),
        Err(astm::ReceiptError::Limit)
    ));
    assert!(
        astm::decode(
            raw.get(..raw.len().saturating_sub(1)).ok_or("fixture")?,
            limit,
            astm::HexPolicy::Strict,
            astm::RecordBoundaries::Separate,
            text.len()
        )
        .is_err()
    );
    let mut outside = raw.clone();
    outside.push(4);
    assert!(matches!(
        astm::decode(
            &outside,
            limit,
            astm::HexPolicy::Strict,
            astm::RecordBoundaries::Separate,
            text.len()
        ),
        Err(astm::ReceiptError::Outside)
    ));
    let mut corrupt = raw;
    *corrupt.get_mut(2).ok_or("fixture")? = b'X';
    assert!(matches!(
        astm::decode(
            &corrupt,
            limit,
            astm::HexPolicy::Strict,
            astm::RecordBoundaries::Separate,
            text.len()
        ),
        Err(astm::ReceiptError::Frame(astm::Error::BadChecksum { .. }))
    ));
    Ok(())
}

// HL7 MLLP R1 §1.2
#[test]
fn retained_block_uses_same_acceptance_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let raw = b"\x0bMSH|synthetic\r\x1c\r";
    let limit = hl7::Limit::new(64)?;
    assert_eq!(hl7::decode(raw, limit)?, b"MSH|synthetic\r");
    let mut trailing = raw.to_vec();
    trailing.extend_from_slice(raw);
    assert!(matches!(
        hl7::decode(&trailing, limit),
        Err(hl7::ReceiptError::Trailing)
    ));
    assert!(matches!(
        hl7::decode(b"\x0bMSH|partial", limit),
        Err(hl7::ReceiptError::Incomplete)
    ));
    assert!(matches!(
        hl7::decode(raw, hl7::Limit::new(2)?),
        Err(hl7::ReceiptError::Frame(hl7::Error::TooLong))
    ));
    Ok(())
}
