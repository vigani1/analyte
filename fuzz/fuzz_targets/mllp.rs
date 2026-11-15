#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_framing::mllp::*;
    let mut decoder = Decoder::new(Limit::new(1_048_576).unwrap());
    let mut retained = Vec::new();
    for &byte in data {
        if let Some(event) = decoder.push(byte) {
            match event {
                Event::Block(raw) | Event::Rejected { raw, .. } => retained.extend(raw),
            }
        }
        assert!(decoder.pending().len() <= 1_048_580);
    }
    if let Some(event) = decoder.finish() {
        match event {
            Event::Block(raw) | Event::Rejected { raw, .. } => retained.extend(raw),
        }
    }
    assert_eq!(retained, data);
});
