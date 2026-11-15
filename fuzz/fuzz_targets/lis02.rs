#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(messages) = analyte_astm::parse(data, Default::default()) {
        let raw: Vec<u8> = messages.iter().flat_map(|m| m.encode()).collect();
        assert_eq!(raw, data);
        let _ = analyte_astm::parse(&raw, Default::default()).unwrap();
    }
});
