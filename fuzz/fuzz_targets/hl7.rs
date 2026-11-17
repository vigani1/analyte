//! Bounded fuzz input for the hl7 parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(message) = analyte_hl7::Message::parse(data, Default::default()) {
        if let Ok(request) = analyte_hl7::query::Request::new(&message) {
            let _ = analyte_hl7::query::specimen(
                request,
                &analyte_hl7::query::SpecimenSelector {
                    field: 8,
                    suffix: b"",
                    filters: &[],
                },
            );
        }
        assert_eq!(message.raw(), data);
        assert_eq!(message.encode(), data);
        analyte_hl7::Message::parse(&message.encode(), Default::default()).unwrap();
    }
});
