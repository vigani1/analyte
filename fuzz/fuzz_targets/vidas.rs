//! Hostile VIDAS packet bytes exercise bounded framing and syntax parsing.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_framing::vidas::*;
    use analyte_vidas::syntax::{Message, SampleLimit};
    for mode in [Mode::Literal, Mode::Alternate] {
    let Ok(config) = Config::new(
        mode,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            hex: analyte_framing::vidas::HexOptions {
                uppercase: true,
                accept_either_case: true,
            },
        },
    ) else {
        // Nonzero 80-byte chunks, <=1920-byte messages, and an allowed empty suffix.
        unreachable!("fuzz target bug: constant VIDAS configuration is invalid");
    };
    let mut decoder = Decoder::new(config);
    for (index, &byte) in data.iter().enumerate() {
        if let Ok(Some(packet)) = decoder.push(byte) {
            assert_eq!(Some(packet.raw.as_slice()), data.get(..=index));
            if let Ok(text) = std::str::from_utf8(&packet.body) {
                if let Ok(message) = Message::parse(text, SampleLimit::Seventeen) {
                    assert_eq!(message.text().as_bytes(), packet.body);
                }
            }
        }
    }
    }
    if let Ok(text) = std::str::from_utf8(data) {
        for limit in [SampleLimit::Twelve, SampleLimit::Seventeen] {
            if let Ok(message) = Message::parse(text, limit) {
                assert_eq!(message.text().as_bytes(), data);
                Message::parse(message.text(), limit).unwrap();
            }
        }
    }
});
