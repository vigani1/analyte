//! Checks configured field mappings and code tables.
use super::*;

pub(super) fn field(field: &Field, syntax: SyntaxKind, path: &str, key: &str) -> Result<(), Error> {
    let record = field.record.as_str();
    let valid_record = match syntax {
        SyntaxKind::AstmLis02 => {
            matches!(record, "H" | "P" | "O" | "R" | "C" | "Q" | "L" | "M" | "S")
        }
        SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser => false,
        SyntaxKind::Hl7V2 => {
            record.len() == 3
                && record
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        }
    };
    if !valid_record
        || field.field == 0
        || field.component == 0
        || field.repetition == 0
        || field.field > 1024
        || field.component > 1024
        || field.repetition > 1024
    {
        return Err(invalid(
            path,
            key,
            "use a record in this syntax and one-based positions no larger than 1024",
        ));
    }
    Ok(())
}

pub(super) fn validate_mappings(m: &MachineDocument, path: &str) -> Result<(), Error> {
    let Some(mapping) = &m.mappings else {
        let codes = m
            .codes
            .as_ref()
            .ok_or_else(|| invalid(path, "codes", "vendor syntax requires code tables"))?;
        return validate_tables(&codes.tests, &codes.units, path);
    };
    validate_component_parts(m, mapping, path)?;
    for (key, len) in [
        ("mappings.tests", mapping.tests.len()),
        ("mappings.units", mapping.units.len()),
        ("mappings.qc", mapping.qc.len()),
        ("mappings.identity.run", mapping.identity.run.len()),
        (
            "mappings.identity.correction_status_codes",
            mapping.identity.correction_status_codes.len(),
        ),
    ] {
        if len > 4096 {
            return Err(invalid(
                path,
                key,
                "limit each mapping table to 4096 entries",
            ));
        }
    }
    field(
        &mapping.sample.field,
        m.syntax.kind,
        path,
        "mappings.sample.field",
    )?;
    for rule in &mapping.qc {
        field(&rule.field, m.syntax.kind, path, "mappings.qc.field")?;
    }
    let identity = &mapping.identity;
    field(
        &identity.order_id,
        m.syntax.kind,
        path,
        "mappings.identity.order_id",
    )?;
    match &identity.order_namespace {
        OrderNamespace::Fixed { fixed: _ } => {}
        OrderNamespace::Field { field: selected } => field(
            selected,
            m.syntax.kind,
            path,
            "mappings.identity.order_namespace.field",
        )?,
    }
    match &identity.sub_id {
        SubId::Field(selected) => field(selected, m.syntax.kind, path, "mappings.identity.sub_id")?,
        SubId::None(NoSubId::None) => {}
    }
    for selected in &identity.run {
        field(selected, m.syntax.kind, path, "mappings.identity.run")?;
    }
    let mut codes = std::collections::HashSet::new();
    for code in &identity.correction_status_codes {
        if !codes.insert(code) {
            return Err(invalid(
                path,
                "mappings.identity.correction_status_codes",
                "remove duplicate correction codes",
            ));
        }
    }
    validate_tables(&mapping.tests, &mapping.units, path)
}

pub(super) fn validate_component_parts(
    m: &MachineDocument,
    mapping: &Mappings,
    path: &str,
) -> Result<(), Error> {
    if let Some(parts) = &mapping.component_parts {
        if m.syntax.kind != SyntaxKind::AstmLis02 {
            return Err(invalid(
                path,
                "mappings.component_parts",
                "requires ASTM LIS02",
            ));
        }
        if parts.separator.is_control() || "|\\^&".contains(parts.separator) {
            return Err(Error::PartSeparator { path: path.into() });
        }
        if !(1..=1024).contains(&parts.component)
            || !(1..=1024).contains(&parts.count)
            || !(1..=parts.count).contains(&parts.code)
            || parts
                .dilution
                .is_some_and(|i| !(1..=parts.count).contains(&i))
        {
            return Err(Error::PartIndex { path: path.into() });
        }
        if let Some(applied) = &parts.applied {
            field(
                &applied.field,
                m.syntax.kind,
                path,
                "mappings.component_parts.applied.field",
            )?;
            if applied.yes == applied.no || parts.dilution.is_none() {
                return Err(invalid(
                    path,
                    "mappings.component_parts.applied",
                    "requires dilution and distinct yes/no codes",
                ));
            }
        }
        if parts.dilution == Some(parts.code) {
            return Err(Error::PartOverlap { path: path.into() });
        }
    }
    Ok(())
}

pub(super) fn validate_tables(
    tests: &[TestMapping],
    units: &[UnitMapping],
    path: &str,
) -> Result<(), Error> {
    if tests.len() > 4096 || units.len() > 4096 {
        return Err(invalid(
            path,
            "codes",
            "limit each code table to 4096 entries",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for test in tests {
        if !seen.insert((&test.machine_system, &test.machine_code)) {
            return Err(invalid(
                path,
                "mappings.tests",
                "remove duplicate source system/code entries",
            ));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for unit in units {
        if !seen.insert(&unit.original) {
            return Err(invalid(
                path,
                "mappings.units",
                "remove duplicate original unit entries",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_mapping_family(m: &MachineDocument, path: &str) -> Result<(), Error> {
    // The permissive TOML document becomes exactly one table family at this boundary.
    match (&m.mappings, &m.codes, m.syntax.kind) {
        (Some(_), None, SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2)
        | (None, Some(_), SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser) => {}
        (
            Some(_),
            Some(_) | None,
            SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser,
        )
        | (None, None, SyntaxKind::Poct | SyntaxKind::VidasLiteral | SyntaxKind::Autolyser) => {
            return Err(Error::VendorFieldMappings { path: path.into() });
        }
        (None, Some(_) | None, SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2)
        | (Some(_), Some(_), SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2) => {
            return Err(invalid(
                path,
                "mappings",
                "record syntax requires field mappings only",
            ));
        }
    }
    Ok(())
}
