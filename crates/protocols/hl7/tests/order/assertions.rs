//! Defines rejection assertions shared by the protocol cases.
macro_rules! bad_order_reply {
    ($name:ident,$body:expr,$pattern:pat) => {
        #[test]
        fn $name() -> Result<(), Error> {
            for version in [Version::V231, Version::V251] {
                let sent = sent(version)?;
                let message = reply(version, b"AA", $body)?;
                assert!(matches!(OrderResponse::new(&message, &sent), Err($pattern)));
            }
            Ok(())
        }
    };
}
