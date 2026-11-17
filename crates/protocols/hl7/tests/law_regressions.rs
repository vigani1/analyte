//! LAW transaction regression cases traced to TF-2b sections 3.27 and C.4-6.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::{
    Message, Options,
    law::{Error, Query},
};
#[expect(
    clippy::expect_used,
    reason = "synthetic fixture construction must succeed"
)]
fn message(body: &str, time: &str) -> Message {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|{time}||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\r{body}\r").as_bytes(), Options { charset: analyte_hl7::Charset::Utf8, ..Options::default() }).expect("synthetic message parses")
}
const QPD: &str = "QPD|WOS^Work Order Step^IHELAW|tag|000Ab";
const RCP: &str = "RCP|I||R^Real Time^HL70394";
#[test]
fn law_tf2b_3_27_4_1_2_1_query_segment_order_and_cardinality() {
    for body in [
        QPD.to_owned(),
        RCP.to_owned(),
        format!("{RCP}\r{QPD}"),
        format!("{QPD}\r{QPD}\r{RCP}"),
        format!("{QPD}\r{RCP}\r{RCP}"),
    ] {
        let m = message(&body, "20260101120000+0000");
        let raw = m.raw().to_vec();
        assert!(Query::new(&m).is_err());
        assert_eq!(m.raw(), raw);
    }
}
#[test]
fn law_tf2b_3_27_4_1_2_4_rcp_immediate_realtime_and_excluded_fields() {
    for (rcp, field) in [
        ("RCP|D||R^^HL70394", 1),
        ("RCP|I", 3),
        ("RCP|I||X^^HL70394", 3),
        ("RCP|I||R^^wrong", 3),
        ("RCP|I||R^wrong^HL70394", 3),
        ("RCP|I||R^^HL70394^extra", 3),
        ("RCP|I||R^^HL70394~R^^HL70394", 3),
        ("RCP|I|1|R^^HL70394", 0),
        ("RCP|I||R^^HL70394|1", 0),
    ] {
        let m = message(&format!("{QPD}\r{rcp}"), "20260101120000+0000");
        assert!(matches!(Query::new(&m), Err(Error::Content{segment:"RCP",field:f}) if f == field));
    }
    assert!(
        Query::new(&message(
            &format!("{QPD}\rRCP|I||R^^HL70394"),
            "20260101120000+0000"
        ))
        .is_ok()
    );
}
#[test]
fn law_tf2b_3_27_4_1_2_3_query_name_fixed_tuples_and_profile_limits() {
    let m = message(
        &format!("{}\r{RCP}", QPD.rewrite("Work Order Step", "wrong")),
        "20260101120000+0000",
    );
    assert!(matches!(
        Query::new(&m),
        Err(Error::Content {
            segment: "QPD",
            field: 1
        })
    ));
}

#[test]
fn law_tf2b_3_27_4_1_2_3_location_requires_identifier_and_vendor_system() {
    for location in [
        "^Bench^99LAB",
        "room^Bench",
        "room^Bench^HL70394",
        "room^Bench^99!",
        "123456789012345678901^Bench^99LAB",
    ] {
        let m = message(
            &format!(
                "QPD|WOS_BY_RACK^Work Order Step by Rack^IHELAW|tag||rack|1|||{location}\r{RCP}"
            ),
            "20260101120000+0000",
        );
        let raw = m.raw().to_vec();
        assert!(matches!(
            Query::new(&m),
            Err(Error::Content {
                segment: "QPD",
                field: 8
            })
        ));
        assert_eq!(m.raw(), raw);
    }
}
#[test]
fn hl7_251_2_a_22_law_c4_6_rejects_impossible_timezone_offsets_preserves_raw() {
    for time in [
        "20260101120000-2359",
        "20260101120000+2359",
        "20260101120000+1401",
    ] {
        let m = message(&format!("{QPD}\r{RCP}"), time);
        let raw = m.raw().to_vec();
        assert!(Query::new(&m).is_err());
        assert_eq!(m.raw(), raw);
    }
}

#[test]
fn hl7_251_2_a_33_2_authority_bindings_refuse_bad_iso_and_unsupported_schemes() {
    use analyte_hl7::law::AuthorityBinding;
    for value in [b"word".as_slice(), b"1..2", b"1.02", b"3.1"] {
        assert!(AuthorityBinding::new(b"Local", value, b"ISO").is_err());
    }
    assert!(AuthorityBinding::new(b"Local", b"1.2.3", b"URI").is_err());
    assert!(AuthorityBinding::new(b"Local", b"1.2.3", b"ISO").is_ok());
}

#[test]
fn law_tf2b_3_27_4_1_2_3_informational_tuple_exceeds_field_table_ceiling() {
    let m = message(
        &format!(
            "QPD|WOS_INFORMATIONAL_RACK^Query by WOS and Informational Rack^IHELAW|tag|tube|rack|1\r{RCP}"
        ),
        "20260101120000+0000",
    );
    assert!(matches!(
        Query::new(&m),
        Err(Error::Content {
            segment: "QPD",
            field: 1
        })
    ));
}
