//! Verifies the shared decoding contract for all six protocol stacks.
use analyte_framing::{astm, autolyser, hl7, poct, vidas};

fn collect<E, X>(
    raw: &[u8],
    mut push: impl FnMut(u8) -> Result<Option<E>, X>,
) -> Result<Vec<E>, X> {
    let mut events = Vec::new();
    for byte in raw {
        if let Some(event) = push(*byte)? {
            events.push(event);
        }
    }
    Ok(events)
}

// ADR 0012; CLSI LIS01-A2; HL7 MLLP; CLSI POCT01-A2; VIDAS BCI Link; Autolyser manual Rev18
#[test]
fn all_six_stacks_share_decode() -> Result<(), Box<dyn std::error::Error>> {
    let raw: Vec<u8> = astm::encode(
        b"R|synthetic\r",
        astm::FrameNumber::FIRST,
        astm::Limit::default(),
    )?
    .flatten()
    .collect();
    assert!(matches!(
        {
            let mut decoder = astm::Decoder::new(astm::Limit::default(), astm::HexPolicy::Strict);
            let events = collect(&raw, |byte| {
                Ok::<_, std::convert::Infallible>(decoder.push(byte))
            })?;
            assert!(decoder.finish().is_none());
            events
        }
        .as_slice(),
        [astm::Event::Frame(_)]
    ));
    let limit = hl7::Limit::new(100)?;
    let raw = hl7::encode(b"synthetic", limit)?;
    let mut decoder = hl7::Decoder::new(limit);
    assert!(matches!(
        collect(&raw, |byte| Ok::<_, std::convert::Infallible>(
            decoder.push(byte)
        ))?
        .as_slice(),
        [hl7::Event::Block(_)]
    ));
    assert!(decoder.finish().is_none());
    let mut decoder = poct::Decoder::new(100, 10);
    assert_eq!(
        collect(b"<x/><y/>", |byte| decoder.push(byte))?,
        [b"<x/>".to_vec(), b"<y/>".to_vec()]
    );
    assert!(decoder.finish()?.is_none());
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
        {
            let mut decoder = vidas::Decoder::new(config);
            let events = collect(&raw, |byte| decoder.push(byte))?;
            assert!(decoder.finish()?.is_none());
            events
        }[0]
        .body,
        b"synthetic"
    );
    let raw = autolyser::encode(b"synthetic".to_vec());
    let frames = {
        let mut decoder = autolyser::Decoder::new(autolyser::Mode::Report, 100)?;
        let events = collect(&raw, |byte| decoder.push(byte))?;
        assert!(decoder.finish().raw().is_empty());
        events
    };
    assert_eq!(frames[0].body()?, b"synthetic");
    Ok(())
}
