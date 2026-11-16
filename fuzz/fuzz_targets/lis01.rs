//! Bounded fuzz input for the lis01 parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_framing::astm::*;
    let mut decoder = Decoder::new(Limit::default(), HexPolicy::Strict);
    let mut retained = Vec::new();
    for &byte in data {
        if let Some(event) = decoder.push(byte) {
            match event {
                Event::Frame(f) => retained.extend_from_slice(f.raw()),
                Event::Rejected { raw, .. } => retained.extend(raw),
                Event::Outside(b) => retained.push(b),
            }
        }
        assert!(decoder.pending().len() <= 64_000);
    }
    if let Some(event) = decoder.finish() {
        match event {
            Event::Frame(f) => retained.extend_from_slice(f.raw()),
            Event::Rejected { raw, .. } => retained.extend(raw),
            Event::Outside(b) => retained.push(b),
        }
    }
    assert_eq!(retained, data);
});
