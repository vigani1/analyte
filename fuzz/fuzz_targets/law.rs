#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(message) = analyte_hl7::law::parse_message(
        data,
        analyte_hl7::Options {
            charset: analyte_hl7::Charset::Utf8,
            ..Default::default()
        },
    ) {
        assert_eq!(message.raw(), data);
        let _ = analyte_hl7::law::Query::new(&message);
        let _ = analyte_hl7::law::Results::new(&message);
        let _ = analyte_hl7::law::WorkResponse::new(&message, &message);
        let _ = analyte_hl7::law::validate_control(
            &message,
            analyte_hl7::law::ControlDirection::Receive,
        );
    }
});
