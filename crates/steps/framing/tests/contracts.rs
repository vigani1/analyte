//! Verifies the shared decoding contract for all six protocol stacks.
use analyte_framing::{Decode, astm, autolyser, hl7, law, poct, vidas};

fn collect<D: Decode>(mut decoder: D, raw: &[u8]) -> Result<Vec<D::Event>, D::Error> {
    let mut events = Vec::new();
    for byte in raw {
        if let Some(event) = decoder.push_byte(*byte)? {
            events.push(event);
        }
    }
    assert!(decoder.finish_stream()?.is_none());
    Ok(events)
}

#[test]
fn adr0012_lis01_mllp_poct_bci_rev18_all_six_stacks_share_decode()
-> Result<(), Box<dyn std::error::Error>> {
    let raw: Vec<u8> = astm::encode(
        b"R|synthetic\r",
        astm::FrameNumber::FIRST,
        astm::Limit::default(),
    )?
    .flatten()
    .collect();
    assert!(matches!(
        collect(
            astm::Decoder::new(astm::Limit::default(), astm::HexPolicy::Strict),
            &raw
        )?
        .as_slice(),
        [astm::Event::Frame(_)]
    ));
    let limit = hl7::Limit::new(100)?;
    let raw = hl7::encode(b"synthetic", limit)?;
    assert!(matches!(
        collect(hl7::Decoder::new(limit), &raw)?.as_slice(),
        [hl7::Event::Block(_)]
    ));
    assert!(matches!(
        collect(law::Decoder::new(limit), &raw)?.as_slice(),
        [law::Event::Block(_)]
    ));
    assert_eq!(
        collect(poct::Decoder::new(100, 10), b"<x/><y/>")?,
        [b"<x/>".to_vec(), b"<y/>".to_vec()]
    );
    let config = vidas::Config::new(
        vidas::Mode::Alternate,
        80,
        1920,
        &[],
        vidas::Checksum {
            include_stx: false,
            include_suffix: false,
            hex: vidas::HexOptions {
                uppercase: true,
                accept_either_case: false,
            },
        },
    )?;
    let raw = config.encode(b"synthetic")?;
    assert_eq!(
        collect(vidas::Decoder::new(config), &raw)?[0].body,
        b"synthetic"
    );
    let raw = autolyser::finish(b"synthetic".to_vec());
    let frames = collect(autolyser::Decoder::new(autolyser::Mode::Report, 100)?, &raw)?;
    assert_eq!(frames[0].body()?, b"synthetic");
    Ok(())
}
