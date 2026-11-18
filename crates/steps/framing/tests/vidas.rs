//! Tests VIDAS checksum placement, suffix boundaries and frame limits.
use analyte_framing::vidas::{Checksum, Config, Decoder, Mode};

fn config(
    mode: Mode,
    suffix: &[u8],
    include_suffix: bool,
) -> Result<Config, Box<dyn std::error::Error>> {
    Ok(Config::new(
        mode,
        3,
        1920,
        suffix,
        Checksum {
            include_stx: false,
            include_suffix,
            hex: analyte_framing::vidas::HexOptions {
                uppercase: false,
                accept_either_case: true,
            },
        },
    )?)
}

// VIDAS BCI Link rev A §3
#[test]
fn checksum_precedes_etx() -> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Alternate, &[], false)?;
    // Independent arithmetic: RS + 'A' + GS = 124 = 0x7c.
    let raw = c.encode(b"A")?;
    assert_eq!(raw, b"\x02\x1eA\x1d7c\x03");
    let mut decoder = Decoder::new(c);
    for byte in b"\x02\x1eA\x1d7c" {
        assert!(decoder.push(*byte)?.is_none());
    }
    let packet = decoder.push(3)?.ok_or("missing packet")?;
    assert_eq!(packet.raw, raw);
    assert_eq!(packet.body, b"A");
    Ok(())
}

// VIDAS BCI Link rev A §3
#[test]
fn every_suffix_and_boundary_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [Mode::Literal, Mode::Alternate] {
        for suffix in [b"".as_slice(), b"\r", b"\n", b"\r\n"] {
            for include in [false, true] {
                let c = config(mode, suffix, include)?;
                let raw = c.encode(b"ci0012|")?;
                let mut decoder = Decoder::new(c);
                let mut packet = None;
                for byte in &raw {
                    packet = decoder.push(*byte)?;
                }
                let packet = packet.ok_or("missing boundary")?;
                assert_eq!(packet.body, b"ci0012|");
                assert_eq!(packet.raw, raw);
            }
        }
    }
    Ok(())
}

// VIDAS BCI Link rev A §3
#[test]
fn bad_checksum_or_oversize_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Literal, &[], false)?;
    let mut d = Decoder::new(c.clone());
    for byte in b"\x02\x1eA\x1d00".iter().take(5) {
        assert!(d.push(*byte)?.is_none());
    }
    assert!(d.push(b'0').is_err());
    assert!(d.push(2).is_err());
    assert!(c.encode(&vec![b'A'; 1921]).is_err());
    Ok(())
}

// VIDAS BCI Link rev A §3
#[test]
fn retained_packet_needs_exact_boundary() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_framing::vidas::{Error, decode};
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, b"\r\n", true)?;
        let raw = c.encode(b"synthetic")?;
        assert_eq!(decode(&raw, c.clone())?.body, b"synthetic");
        assert!(matches!(
            decode(
                raw.get(..raw.len().saturating_sub(1)).ok_or("fixture")?,
                c.clone()
            ),
            Err(Error::Incomplete)
        ));
        let mut trailing = raw.clone();
        trailing.push(4);
        assert!(matches!(decode(&trailing, c), Err(Error::Trailing)));
    }
    Ok(())
}
