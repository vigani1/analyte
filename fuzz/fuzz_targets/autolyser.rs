#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_autolyser::{dialog::*, records::*};
    use std::time::Duration;
    const CONFIG: Config = Config {
        poll: Duration::from_secs(1),
        ack: Duration::from_secs(1),
        response: Duration::from_secs(10),
        inter_byte: Duration::from_secs(1),
        storage: Duration::from_secs(1),
        max_bytes: 11_038,
        controls: false,
        recovery: Recovery::ManualLast,
    };
    // Dialog::new requires nonzero deadlines and 2..=11038 response bytes.
    let Ok(mut dialog) = Dialog::new(CONFIG) else {
        unreachable!("fuzz target bug: constant Autolyser configuration is invalid");
    };
    let _ = dialog.step(Duration::ZERO, Event::Poll(Request::Next));
    let _ = dialog.step(Duration::ZERO, Event::Byte(6));
    for &byte in data {
        let _ = dialog.step(Duration::ZERO, Event::Byte(byte));
    }
    if let Ok(worklist) = Worklist::parse(data) {
        assert_eq!(worklist.encode(), data);
    }
    for controls in [false, true] {
        if let Ok(report) = Report::parse(data, controls) {
            assert_eq!(report.raw, data);
        }
    }
});
