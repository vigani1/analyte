//! Tests configured field selectors and vendor syntax rejection.
use analyte_config::StackChoice;
use analyte_mapping::{FieldSyntax, UnsupportedFieldSyntax};

// ADR 0002
#[test]
fn field_syntax_rejects_vendor_families() -> Result<(), UnsupportedFieldSyntax> {
    assert_eq!(
        FieldSyntax::try_from(StackChoice::AstmLis02)?,
        FieldSyntax::AstmLis02
    );
    assert_eq!(
        FieldSyntax::try_from(StackChoice::Hl7V2)?,
        FieldSyntax::Hl7V2
    );
    for syntax in [
        StackChoice::Poct,
        StackChoice::VidasLiteral,
        StackChoice::Autolyser,
    ] {
        assert!(
            matches!(FieldSyntax::try_from(syntax), Err(UnsupportedFieldSyntax(found)) if found == syntax)
        );
    }
    Ok(())
}
