//! Checks configured field mappings, the machine's code system and unit tables.
use super::{
    Error, Field, Mappings, NoSubId, OrderNamespace, SubId, UnitMapping, ValueExpectation,
    ValueKind, invalid,
};
use crate::{ParsedMachine, StackChoice};

pub(super) fn field(
    field: &Field,
    syntax: StackChoice,
    path: &str,
    key: &str,
) -> Result<(), Error> {
    let record = field.record.as_str();
    let valid_record = if syntax.has_astm_records() {
        matches!(record.as_bytes(), [byte] if byte.is_ascii_uppercase() && analyte_astm::Kind::parse(*byte).is_some())
    } else if syntax.has_hl7_records() {
        record.len() == 3
            && record
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    } else {
        false
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

pub(super) fn validate_mappings(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    let Some(mapping) = &m.mappings else {
        let codes = m
            .codes
            .as_ref()
            .ok_or_else(|| invalid(path, "codes", "vendor syntax requires code tables"))?;
        if codes.vidas_identity.is_some() && m.choice() != StackChoice::VidasLiteral {
            return Err(invalid(
                path,
                "codes.vidas_identity",
                "requires VIDAS Literal",
            ));
        }
        return validate_tables(&[], &codes.units, path);
    };
    validate_test_component(m, mapping, path)?;
    validate_component_parts(m, mapping, path)?;
    if let Some(profile) = &mapping.astm_results {
        validate_astm_results(m, mapping, path, profile.layout)?;
    }
    for (key, len) in [
        ("mappings.value_kinds", mapping.value_kinds.len()),
        ("mappings.units", mapping.units.len()),
        ("mappings.qc", mapping.qc.len()),
        ("mappings.identity.run", mapping.identity.run.len()),
        (
            "mappings.identity.correction_status_codes",
            mapping.identity.correction_status_codes.len(),
        ),
        (
            "mappings.identity.rerun_status_codes",
            mapping.identity.rerun_status_codes.len(),
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
        m.choice(),
        path,
        "mappings.sample.field",
    )?;
    for rule in &mapping.qc {
        field(&rule.field, m.choice(), path, "mappings.qc.field")?;
    }
    let identity = &mapping.identity;
    field(
        &identity.order_id,
        m.choice(),
        path,
        "mappings.identity.order_id",
    )?;
    match &identity.order_namespace {
        OrderNamespace::Fixed { fixed: _ } => {}
        OrderNamespace::Field { field: selected } => field(
            selected,
            m.choice(),
            path,
            "mappings.identity.order_namespace.field",
        )?,
    }
    match &identity.sub_id {
        SubId::Field(selected) => field(selected, m.choice(), path, "mappings.identity.sub_id")?,
        SubId::None(NoSubId::None) => {}
    }
    for selected in &identity.run {
        field(selected, m.choice(), path, "mappings.identity.run")?;
    }
    validate_identity_policy(identity, m.choice(), path)?;
    validate_tables(&mapping.value_kinds, &mapping.units, path)
}

fn validate_identity_policy(
    identity: &crate::IdentityEvidence,
    choice: StackChoice,
    path: &str,
) -> Result<(), Error> {
    let rerun_key = "mappings.identity.rerun_status_codes";
    if choice == StackChoice::LawManager
        && (identity.evidence.is_some() || !identity.rerun_status_codes.is_empty())
    {
        return Err(invalid(
            path,
            "mappings.identity",
            "LAW uses its standard identity and status rules",
        ));
    }
    if !identity.rerun_status_codes.is_empty() && identity.evidence.is_none() {
        return Err(invalid(
            path,
            rerun_key,
            "requires a manual citation under mappings.identity.evidence",
        ));
    }
    if identity.evidence.is_some() && identity.run.is_empty() {
        return Err(invalid(
            path,
            "mappings.identity.run",
            "a documented recipe requires at least one run selector",
        ));
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
    for code in &identity.rerun_status_codes {
        if !codes.insert(code) {
            return Err(invalid(
                path,
                rerun_key,
                "rerun codes must be unique and distinct from correction codes",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_component_parts(
    m: &ParsedMachine<'_>,
    mapping: &Mappings,
    path: &str,
) -> Result<(), Error> {
    if let Some(parts) = &mapping.component_parts {
        if !m.choice().has_astm_records() || mapping.test_component.is_none() {
            return Err(invalid(
                path,
                "mappings.component_parts",
                "requires ASTM LIS02 and mappings.test_component",
            ));
        }
        if parts.separator.is_control() || "|\\^&".contains(parts.separator) {
            return Err(Error::PartSeparator { path: path.into() });
        }
        if !(1..=1024).contains(&parts.count)
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
                m.choice(),
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

// LIS02 section 5.6.1 names where the code is; HL7 OBX-3 and vendor
// messages have one fixed place, so only ASTM declares it.
fn validate_test_component(
    m: &ParsedMachine<'_>,
    mapping: &Mappings,
    path: &str,
) -> Result<(), Error> {
    if m.choice().has_astm_records() != mapping.test_component.is_some() {
        return Err(invalid(
            path,
            "mappings.test_component",
            "name the R.3 component (universal or manufacturer) for ASTM, and only for ASTM",
        ));
    }
    Ok(())
}

pub(super) fn validate_tables(
    kinds: &[ValueExpectation],
    units: &[UnitMapping],
    path: &str,
) -> Result<(), Error> {
    if kinds.len() > 4096 || units.len() > 4096 {
        return Err(invalid(
            path,
            "codes",
            "limit each code table to 4096 entries",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for kind in kinds {
        if kind.kind == ValueKind::Either {
            return Err(invalid(
                path,
                "mappings.value_kinds",
                "expect numeric or coded; leave out codes with no expectation",
            ));
        }
        if !seen.insert((&kind.system, &kind.code)) {
            return Err(invalid(
                path,
                "mappings.value_kinds",
                "remove duplicate system/code entries",
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

pub(super) fn validate_mapping_family(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    // The permissive TOML document becomes exactly one table family at this boundary.
    match (&m.mappings, &m.codes, m.choice().has_field_mappings()) {
        (Some(_), None, true) | (None, Some(_), false) => {}
        (Some(_), Some(_) | None, false) | (None, None, false) => {
            return Err(Error::VendorFieldMappings { path: path.into() });
        }
        (None, Some(_) | None, true) | (Some(_), Some(_), true) => {
            return Err(invalid(
                path,
                "mappings",
                "record syntax requires field mappings only",
            ));
        }
    }
    Ok(())
}

fn validate_astm_results(
    machine: &ParsedMachine<'_>,
    mapping: &Mappings,
    path: &str,
    layout: crate::AstmResultLayout,
) -> Result<(), Error> {
    use crate::AstmResultLayout::{
        CaretDilutionAndAlarm, MeasurementMarker, SlashDilutionAndAlarmCode,
        SlashDilutionAndAlarmCodeWithQcLot,
    };
    let key = "mappings.astm_results";
    if !machine.choice().has_astm_records()
        || mapping.test_component != Some(crate::AstmTestComponent::Manufacturer)
        || mapping.component_parts.is_some()
    {
        return Err(invalid(
            path,
            key,
            "requires ASTM with the manufacturer test component and no separate component decomposition",
        ));
    }
    if matches!(
        layout,
        CaretDilutionAndAlarm | SlashDilutionAndAlarmCode | SlashDilutionAndAlarmCodeWithQcLot
    ) {
        let field = &mapping.sample.field;
        if field.group != crate::Group::Order
            || field.record.as_str() != "O"
            || field.field != 3
            || field.component != 1
            || field.repetition != 1
        {
            return Err(invalid(path, key, "requires O.3.1 as the sample selector"));
        }
    }
    if layout == MeasurementMarker {
        let field = &mapping.sample.field;
        if field.group != crate::Group::Patient
            || field.record.as_str() != "P"
            || field.field != 4
            || field.component != 1
            || field.repetition != 1
        {
            return Err(invalid(path, key, "requires P.4.1 as the sample selector"));
        }
    }
    if !mapping.qc.is_empty() {
        return Err(invalid(
            path,
            key,
            "material classification is determined by the selected upload layout",
        ));
    }
    if !mapping.identity.correction_status_codes.is_empty() {
        return Err(invalid(
            path,
            key,
            "this upload layout documents no result correction code",
        ));
    }
    if mapping.identity.rerun_status_codes.iter().any(|code| {
        matches!(layout, MeasurementMarker | CaretDilutionAndAlarm) || code.as_str() != "C"
    }) {
        return Err(invalid(
            path,
            key,
            "only the slash-separated upload documents a rerun code, C",
        ));
    }
    Ok(())
}
