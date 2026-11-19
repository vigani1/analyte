//! Checks the protected open record's spellings.

use super::*;

#[test]
fn serial_recording_spellings_are_compatible() -> Result<(), Error> {
    for (bits, parity, stops, flow, timeout, expected) in [
        (
            DataBits::Five,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            Duration::from_millis(1),
            "Settings { name: \"COM1\", baud: 9600, bits: Five, parity: None, stops: One, flow: None, timeout: 1ms }",
        ),
        (
            DataBits::Six,
            Parity::Odd,
            StopBits::Two,
            FlowControl::Software,
            Duration::from_micros(1500),
            "Settings { name: \"COM1\", baud: 9600, bits: Six, parity: Odd, stops: Two, flow: Software, timeout: 1.5ms }",
        ),
        (
            DataBits::Seven,
            Parity::Even,
            StopBits::One,
            FlowControl::Hardware,
            Duration::from_nanos(1_000_001),
            "Settings { name: \"COM1\", baud: 9600, bits: Seven, parity: Even, stops: One, flow: Hardware, timeout: 1.000001ms }",
        ),
        (
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            Duration::from_secs(1),
            "Settings { name: \"COM1\", baud: 9600, bits: Eight, parity: None, stops: One, flow: None, timeout: 1s }",
        ),
    ] {
        let settings = Settings::new(
            Device::Named("COM1".into()),
            9600,
            bits,
            parity,
            stops,
            flow,
            timeout,
        )?;
        let opened = Opened {
            settings: &settings,
            name: "COM1",
        };
        assert_eq!(opened.to_string(), expected);
    }
    Ok(())
}

// Plan §11
#[test]
fn usb_open_record_names_port_and_serial() -> Result<(), Error> {
    let settings = Settings::new(
        Device::UsbSerial("A10K3XYZ".into()),
        9600,
        DataBits::Eight,
        Parity::None,
        StopBits::One,
        FlowControl::None,
        Duration::from_secs(1),
    )?;
    let opened = Opened {
        settings: &settings,
        name: "COM12",
    };
    assert_eq!(
        opened.to_string(),
        "Settings { name: \"COM12\", usb_serial: \"A10K3XYZ\", baud: 9600, bits: Eight, parity: None, stops: One, flow: None, timeout: 1s }"
    );
    Ok(())
}
