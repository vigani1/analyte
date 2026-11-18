//! Tests POCT XML limits and source preservation.
use analyte_poct::{Document, Error, Limits};
use proptest::prelude::*;

fn limits() -> Limits {
    Limits {
        message_bytes: 4096,
        depth: 8,
        elements: 32,
        attribute_bytes: 128,
    }
}

// CLSI POCT01-A2 §B.8
#[test]
fn xml_preserves_bytes_and_decodes_attributes() -> Result<(), Error> {
    let raw = b"<synthetic V='001&amp;Ab'><value V='1.230'/></synthetic>\r\n";
    let document = Document::parse(raw, limits())?;
    assert_eq!(document.raw(), raw);
    let child = document.elements().get(1).ok_or(Error::Root)?;
    assert_eq!(child.name(), "value");
    assert_eq!(child.parent(), Some(0));
    let root = document.elements().first().ok_or(Error::Root)?;
    assert_eq!(
        root.attributes().first(),
        Some(&("V".to_owned(), "001&Ab".to_owned()))
    );
    Ok(())
}

// CLSI POCT01-A2; project security rule
#[test]
fn rejects_dtd_without_resolving_it() {
    for raw in [
        "<!DOCTYPE x SYSTEM 'file:///missing'><x/>",
        "<!DOCTYPE x SYSTEM 'https://invalid.example/a'><x/>",
        "<!DOCTYPE x [<!ENTITY a 'text'>]><x>&a;</x>",
    ] {
        assert!(matches!(
            Document::parse(raw.as_bytes(), limits()),
            Err(Error::Doctype)
        ));
    }
}

// CLSI POCT01-A2; project security rule
#[test]
fn bounds_each_resource() {
    let raw = b"<x V='123'><y/></x>";
    let mut bound = limits();
    bound.message_bytes = 1;
    assert!(matches!(
        Document::parse(raw, bound),
        Err(Error::MessageLimit)
    ));
    bound = limits();
    bound.depth = 1;
    assert!(matches!(
        Document::parse(raw, bound),
        Err(Error::DepthLimit)
    ));
    bound = limits();
    bound.elements = 1;
    assert!(matches!(
        Document::parse(raw, bound),
        Err(Error::ElementLimit)
    ));
    bound = limits();
    bound.attribute_bytes = 2;
    assert!(matches!(
        Document::parse(raw, bound),
        Err(Error::AttributeLimit)
    ));
}

// CLSI POCT01-A2; project security rule
#[test]
fn rejects_malformed_documents() {
    for raw in [
        "",
        "<x>",
        "<x></y>",
        "<x/><y/>",
        "<x V='a' V='b'/>",
        "<x>&unknown;</x>",
        "<x V='&#0;'/>",
        "<x>&#0;</x>",
        "<![CDATA[outside]]><x/>",
        "<?xml version='1.1'?><x/>",
        "<?xml version='1.0' encoding='UTF-16'?><x/>",
        "<x/><?tool data?>",
        "<x>\0</x>",
        "outside<x/>",
        "<1bad/>",
        "<x><!--bad--comment--></x>",
        "<x V='a<b'/>",
    ] {
        assert!(
            Document::parse(raw.as_bytes(), limits()).is_err(),
            "accepted {raw}"
        );
    }
}

// CLSI POCT01-A2 §B.8
#[test]
fn encapsulated_text_is_not_an_attribute() -> Result<(), Error> {
    let document = Document::parse(
        b"<?xml version='1.0'?><data>one&amp;two<![CDATA[<three>]]></data>",
        limits(),
    )?;
    assert_eq!(
        document.elements().first().ok_or(Error::Root)?.text(),
        "one&two<three>"
    );
    assert_eq!(format!("{document:?}"), "Document { .. }");
    Ok(())
}

proptest! {
    // CLSI POCT01-A2; project security rule
    #[test]
    fn arbitrary_bytes_do_not_panic(raw in prop::collection::vec(any::<u8>(), 0..5000)) {
        if let Ok(document) = Document::parse(&raw, limits()) {
            prop_assert_eq!(document.raw(), raw.as_slice());
            prop_assert!(document.elements().len() <= limits().elements);
        }
    }

    // CLSI POCT01-A2 §B.8
    #[test]
    fn written_attribute_text_survives(value in "[a-zA-Z0-9 .-]{0,100}") {
        let raw = format!("<synthetic V='{value}'/>");
        let document = Document::parse(raw.as_bytes(), limits())?;
        let root = document.elements().first().ok_or(Error::Root)?;
        prop_assert_eq!(root.attributes().first().map(|(_, v)| v.as_str()), Some(value.as_str()));
    }
}

// CLSI POCT01-A2; project rule
#[test]
fn bom_accepted_and_bad_declarations_refused() -> Result<(), Error> {
    let raw = b"\xef\xbb\xbf<?xml version='1.0' encoding='UTF-8' standalone='yes'?><synthetic/>";
    assert_eq!(Document::parse(raw, limits())?.raw(), raw);
    for raw in [
        "<?xml version='1.0' unknown='x'?><x/>",
        "<?xml version='1.0' standalone='maybe'?><x/>",
        "<x>]]></x>",
    ] {
        assert!(Document::parse(raw.as_bytes(), limits()).is_err());
    }
    Ok(())
}
