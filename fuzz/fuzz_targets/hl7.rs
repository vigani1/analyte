#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(message) = analyte_hl7::Message::parse(data, Default::default()) {
        assert_eq!(message.raw(), data);
        assert_eq!(message.serialize(), data);
        analyte_hl7::Message::parse(&message.serialize(), Default::default()).unwrap();
    }
});
