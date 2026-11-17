//! Defines rejection assertions shared by the protocol cases.
macro_rules! negative {
    ($name:ident,$segment:literal,$field:literal,$value:expr) => {
        #[test]
        fn $name() -> Result<(), Error> {
            for edition in ["2.3.1", "2.5.1"] {
                let message = fixture(edition, $segment, $field, $value)?;
                assert!(
                    matches!(
                        Request::new(&message),
                        Err(Error::Control {
                            segment: $segment,
                            field: $field
                        })
                    ),
                    "wrong outcome for {}",
                    edition
                );
            }
            Ok(())
        }
    };
}
