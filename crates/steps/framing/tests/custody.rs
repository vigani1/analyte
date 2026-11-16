//! Synthetic evidence-retention checks from ADR 0003 and MLLP Release 1.
//! MLLP R1 §1.2 defines framing/repertoire. Size ceilings come from rule 7;
//! disconnect evidence and reset behavior come from ADR 0003, not MLLP timers.
use analyte_framing::mllp as m;

#[test]
fn rule7_mllp_r1_1_2_encoder_payload_limit_is_inclusive() -> Result<(), m::Error> {
    let limit = m::Limit::new(2)?;
    assert_eq!(m::encode(b"A", limit)?, b"\x0bA\x1c\r");
    assert_eq!(m::encode(b"AB", limit)?, b"\x0bAB\x1c\r");
    assert_eq!(m::encode(b"ABC", limit), Err(m::Error::TooLong));
    Ok(())
}

#[test]
fn mllp_r1_1_2_payload_repertoire_boundary_excludes_unit_separator() -> Result<(), m::Error> {
    let limit = m::Limit::new(1)?;
    for byte in [0x1f, 0x20, 0xff, m::CR] {
        let mut decoder = m::Decoder::new(limit);
        assert_eq!(decoder.push(m::SB), None);
        if byte == 0x1f {
            assert_eq!(m::encode(&[byte], limit), Err(m::Error::InvalidByte(byte)));
            assert_eq!(
                decoder.push(byte),
                Some(m::Event::Rejected {
                    error: m::Error::InvalidByte(byte),
                    raw: vec![m::SB, byte],
                })
            );
        } else {
            let expected = vec![m::SB, byte, m::EB, m::CR];
            assert_eq!(m::encode(&[byte], limit)?, expected);
            assert_eq!(decoder.push(byte), None);
            assert_eq!(decoder.push(m::EB), None);
            assert_eq!(decoder.push(m::CR), Some(m::Event::Block(expected)));
        }
    }
    Ok(())
}

#[test]
fn adr0003_mllp_r1_1_2_disconnect_retains_partial_block_and_resets() -> Result<(), m::Error> {
    for wire in [b"\x0b".as_slice(), b"\x0bABC", b"\x0bABC\x1c"] {
        let mut decoder = m::Decoder::new(m::Limit::new(3)?);
        for byte in wire {
            assert_eq!(decoder.push(*byte), None);
        }
        assert_eq!(decoder.pending(), wire);
        assert_eq!(
            decoder.finish(),
            Some(m::Event::Rejected {
                error: m::Error::Incomplete,
                raw: wire.to_vec(),
            })
        );
        assert!(decoder.pending().is_empty());
        assert_eq!(decoder.finish(), None);
        assert_eq!(
            decoder.push(b'X'),
            Some(m::Event::Rejected {
                error: m::Error::Outside,
                raw: vec![b'X'],
            })
        );
        let events: Vec<_> = b"\x0bABC\x1c\r"
            .iter()
            .filter_map(|byte| decoder.push(*byte))
            .collect();
        assert_eq!(events, vec![m::Event::Block(b"\x0bABC\x1c\r".to_vec())]);
    }
    Ok(())
}
