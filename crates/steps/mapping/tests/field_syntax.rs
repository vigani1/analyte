//! Tests adr0002 field syntax rejects every vendor family.
use analyte_config::SyntaxKind;
use analyte_mapping::{FieldSyntax, UnsupportedFieldSyntax};

#[test]
fn adr0002_field_syntax_rejects_every_vendor_family() -> Result<(), UnsupportedFieldSyntax> {
    assert_eq!(
        FieldSyntax::try_from(SyntaxKind::AstmLis02)?,
        FieldSyntax::AstmLis02
    );
    assert_eq!(
        FieldSyntax::try_from(SyntaxKind::Hl7V2)?,
        FieldSyntax::Hl7V2
    );
    for syntax in [
        SyntaxKind::Poct,
        SyntaxKind::VidasLiteral,
        SyntaxKind::Autolyser,
    ] {
        assert!(
            matches!(FieldSyntax::try_from(syntax), Err(UnsupportedFieldSyntax(found)) if found == syntax)
        );
    }
    Ok(())
}
