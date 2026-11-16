//! Bounded fuzz input for the poct parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _reference = analyte_poct::hello_error_reference(
        data,
        analyte_poct::Limits {
            message_bytes: 1_048_576,
            depth: 32,
            elements: 16_384,
            attribute_bytes: 4096,
        },
    );
    if let Ok(document) = analyte_poct::Document::parse(
        data,
        analyte_poct::Limits {
            message_bytes: 1_048_576,
            depth: 32,
            elements: 16_384,
            attribute_bytes: 4096,
        },
    ) {
        assert_eq!(document.raw(), data);
        analyte_poct::Document::parse(
            document.raw(),
            analyte_poct::Limits {
                message_bytes: 1_048_576,
                depth: 32,
                elements: 16_384,
                attribute_bytes: 4096,
            },
        )
        .unwrap();
    }
});
