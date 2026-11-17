//! Bounded fuzz input for the law parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = analyte_hl7::law::RetainedControl::parse(text);
    }
    if let Ok(message) = analyte_hl7::law::parse_message(
        data,
        analyte_hl7::Options {
            charset: analyte_hl7::Charset::Utf8,
            ..Default::default()
        },
    ) {
        let _ = analyte_hl7::law::returned_work(&message);
        let _ = analyte_hl7::law::CustodyHeader::new(&message);
        for record in analyte_hl7::law::custody_records(&message) {
            if let analyte_hl7::law::CustodyRecord::Work { placer } = record {
                let _ = placer;
            }
        }
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
