//! Synthetic LAW query regression cases; no analyzer or app is contacted.
use analyte_hl7::{
    Message, Options,
    law::{Error, Query, Scope},
};
use proptest::prelude::*;

fn query(body: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL|||||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r").as_bytes(), Options::default())
}

#[test]
fn law_tf2b_3_27_4_container_preserves_leading_zero_and_raw()
-> Result<(), Box<dyn std::error::Error>> {
    let message = query("QPD|WOS^Work Order Step^IHELAW|tag|000Ab")?;
    let view = Query::new(&message)?;
    let Scope::Container(id) = view.scope() else {
        return Err("wrong scope".into());
    };
    assert_eq!(id.raw(), b"000Ab");
    assert_eq!(view.tag(), b"tag");
    assert_eq!(view.message().raw(), message.raw());
    Ok(())
}

#[test]
fn law_tf2b_3_27_4_query_forms_and_conditional_fields() -> Result<(), Box<dyn std::error::Error>> {
    for (body, expected) in [
        ("QPD|WOS_ALL^Work Order Step All^IHELAW|tag", "all"),
        (
            "QPD|WOS_BY_RACK^Work Order Step by rack^IHELAW|tag||rack|01",
            "rack",
        ),
        (
            "QPD|WOS_BY_TRAY^Work Order Step by tray^IHELAW|tag||||tray|1^2",
            "tray",
        ),
        (
            "QPD|WOS_BY_ISOLATE^Work Order Step by isolate^IHELAW|tag|child||||||parent",
            "isolate",
        ),
        (
            "QPD|WOS_INFORMATIONAL_RACK^Query^IHELAW|tag|tube|rack|2",
            "informational",
        ),
    ] {
        let message = query(body)?;
        let name = match Query::new(&message)?.scope() {
            Scope::AllPending => "all",
            Scope::Rack { .. } => "rack",
            Scope::Tray { .. } => "tray",
            Scope::Isolate { .. } => "isolate",
            Scope::InformationalRack { .. } => "informational",
            Scope::Container(_) => "container",
        };
        assert_eq!(name, expected);
    }
    Ok(())
}

#[test]
fn law_tf2b_3_27_4_forbidden_missing_null_and_repeated_parameters_retain_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    for body in [
        "QPD|WOS_ALL^Work Order Step All^IHELAW|tag|forbidden",
        "QPD|WOS^Work Order Step^IHELAW|tag",
        "QPD|WOS^Work Order Step^IHELAW|tag|\"\"",
        "QPD|WOS^Work Order Step^IHELAW|tag|a~b",
        "QPD|WOS_BY_RACK^Rack^IHELAW|tag||rack|x",
    ] {
        let message = query(body)?;
        let original = message.serialize();
        assert!(matches!(Query::new(&message), Err(Error::Content { .. })));
        assert_eq!(message.raw(), original);
    }
    Ok(())
}

proptest! {
    #[test]
    fn law_tf2b_3_27_4_bounded_container_strings(id in "[0-9A-Za-z]{1,50}") {
        let message = query(&format!("QPD|WOS^Work Order Step^IHELAW|tag|{id}"))?;
        let parsed = Query::new(&message)?;
        prop_assert!(matches!(parsed.scope(), Scope::Container(_)));
        if let Scope::Container(identity) = parsed.scope() {
            prop_assert_eq!(identity.raw(), id.as_bytes());
        }
    }
}
