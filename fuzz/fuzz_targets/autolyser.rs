//! Bounded fuzz input for the autolyser parser.
#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_autolyser::syntax::*;
    use analyte_dialog::autolyser::*;
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
    let _ = dialog.step(Duration::ZERO, Event::Byte(analyte_framing::autolyser::ACK));
    for &byte in data {
        let _ = dialog.step(Duration::ZERO, Event::Byte(byte));
    }
    for mode in [
        analyte_framing::autolyser::Mode::Report,
        analyte_framing::autolyser::Mode::Insertion,
    ] {
        let mut decoder = analyte_framing::autolyser::Decoder::new(mode, 11_038)
            .expect("constant byte ceiling is valid");
        for &byte in data.iter().take(11_039) {
            match decoder.push(byte) {
                Ok(Some(frame)) => {
                    assert!(frame.raw().len() <= 11_038);
                    let _ = frame.body();
                    break;
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
        let _ = decoder.finish();
    }
    if let Ok(worklist) = Worklist::parse(data) {
        assert_eq!(worklist.encode(), data);
    }
    if let Ok(body) = analyte_framing::autolyser::decode(data) {
        if let Ok(worklist) = Worklist::parse(body) {
            assert_eq!(analyte_framing::autolyser::encode(worklist.encode()), data);
        }
        for controls in [false, true] {
            if let Ok(report) = Report::parse(body, controls) {
                assert_eq!(analyte_framing::autolyser::encode(report.raw), data);
            }
        }
    }
    for controls in [false, true] {
        if let Ok(report) = Report::parse(data, controls) {
            assert_eq!(report.raw, data);
        }
    }
});
