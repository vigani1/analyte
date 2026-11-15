#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_vidas::{
        framing::*,
        syntax::{Message, SampleLimit},
    };
    let config = Config::new(
        Mode::Literal,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            uppercase: true,
            accept_either_case: true,
        },
    )
    .unwrap();
    let mut decoder = Decoder::new(config);
    for &byte in data {
        let _ = decoder.push(byte);
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
