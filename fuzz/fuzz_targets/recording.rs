//! Bounded fuzz input for the recording parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_transport::recording::*;
    if let Ok((header, events)) = read(data, Limits::default()) {
        let mut encoded = Vec::new();
        write_header(&mut encoded, &header, Limits::default()).unwrap();
        for event in &events {
            write_event(&mut encoded, event, Limits::default()).unwrap();
        }
        let (again, replay) = read(encoded.as_slice(), Limits::default()).unwrap();
        assert_eq!(header, again);
        assert_eq!(events, replay);
    }
});
