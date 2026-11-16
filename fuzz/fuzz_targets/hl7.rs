//! Bounded fuzz input for the hl7 parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(message) = analyte_hl7::Message::parse(data, Default::default()) {
        assert_eq!(message.raw(), data);
        assert_eq!(message.encode(), data);
        analyte_hl7::Message::parse(&message.encode(), Default::default()).unwrap();
    }
});
