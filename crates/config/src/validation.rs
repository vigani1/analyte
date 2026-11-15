//! Semantic checks at the config boundary.
use crate::{Error, invalid, schema::*};

pub(super) fn validate_site(site: &Site, transport: &Transport, path: &str) -> Result<(), Error> {
    if site.destination.freshness_seconds == 0 {
        return Err(invalid(
            path,
            "site.destination.freshness_seconds",
            "must be at least one second",
        ));
    }
    let endpoint = &site.endpoint;
    if endpoint.is_empty() || endpoint.len() > 1024 || endpoint.chars().any(char::is_control) {
        return Err(invalid(
            path,
            "site.endpoint",
            "supply a nonempty endpoint of at most 1024 bytes without control characters",
        ));
    }
    match transport.kind {
        TransportKind::TcpClient | TransportKind::TcpServer => {
            let address = endpoint.parse::<std::net::SocketAddr>().map_err(|_| {
                invalid(
                    path,
                    "site.endpoint",
                    "supply an explicit numeric IP address and nonzero port",
                )
            })?;
            if address.port() == 0
                || (transport.kind == TransportKind::TcpClient && address.ip().is_unspecified())
            {
                return Err(invalid(
                    path,
                    "site.endpoint",
                    "use a nonzero port and a concrete client destination",
                ));
            }
        }
        TransportKind::WatchedFolder => {
            if !std::path::Path::new(endpoint).is_absolute() {
                return Err(invalid(
                    path,
                    "site.endpoint",
                    "supply an absolute watched-folder path",
                ));
            }
        }
        TransportKind::Serial => {}
    }
    let url = &site.destination.base_url;
    let authority = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next());
    if url.len() > 2048
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
        || url.contains(['@', '?', '#', '\\'])
        || authority.is_none_or(|a| a.is_empty() || a.starts_with(':'))
    {
        return Err(invalid(
            path,
            "site.destination.base_url",
            "use an HTTPS URL with a host, without credentials, query or fragment",
        ));
    }
    Ok(())
}

fn field(field: &Field, syntax: SyntaxKind, path: &str, key: &str) -> Result<(), Error> {
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

pub(super) fn validate_machine(m: &MachineDocument, path: &str) -> Result<(), Error> {
    crate::law::validate(m, path)?;
    if let Some(orders) = &m.orders {
        orders.validate(m, path)?;
    }
    let t = &m.transport;
    if !(1..=300_000).contains(&t.operation_timeout_ms) {
        return Err(invalid(
            path,
            "transport.operation_timeout_ms",
            "use 1 through 300000 milliseconds",
        ));
    }
    if (t.kind == TransportKind::Serial) != t.serial.is_some() {
        return Err(invalid(
            path,
            "transport.serial",
            "provide full serial settings only for serial transport",
        ));
    }
    if let Some(s) = &t.serial {
        if !(1..=4_000_000).contains(&s.baud) {
            return Err(invalid(
                path,
                "transport.serial.baud",
                "use a baud rate from 1 through 4000000",
            ));
        }
        if !(5..=8).contains(&s.data_bits) {
            return Err(invalid(
                path,
                "transport.serial.data_bits",
                "use 5 through 8 data bits",
            ));
        }
        if !(1..=2).contains(&s.stop_bits) {
            return Err(invalid(
                path,
                "transport.serial.stop_bits",
                "use one or two stop bits",
            ));
        }
    }
    let f = &m.framing;
    let d = &m.dialog;
    let s = &m.syntax;
    let compatible = match d.kind {
        DialogKind::AstmLink => {
            t.kind != TransportKind::WatchedFolder
                && f.kind == FramingKind::AstmLis01
                && s.kind == SyntaxKind::AstmLis02
        }
        DialogKind::Hl7Receiver | DialogKind::LawManager => {
            matches!(t.kind, TransportKind::TcpClient | TransportKind::TcpServer)
                && f.kind == FramingKind::Mllp
                && s.kind == SyntaxKind::Hl7V2
        }
        DialogKind::Poct => {
            matches!(t.kind, TransportKind::TcpClient | TransportKind::TcpServer)
                && f.kind == FramingKind::Poct
                && s.kind == SyntaxKind::Poct
        }
        DialogKind::VidasLiteral => {
            t.kind != TransportKind::WatchedFolder
                && f.kind == FramingKind::VidasLiteral
                && s.kind == SyntaxKind::VidasLiteral
        }
        DialogKind::Autolyser => {
            t.kind != TransportKind::WatchedFolder
                && f.kind == FramingKind::Autolyser
                && s.kind == SyntaxKind::Autolyser
        }
        DialogKind::FilePickup => {
            t.kind == TransportKind::WatchedFolder
                && f.kind == FramingKind::None
                && matches!(s.kind, SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2)
        }
    };
    if !compatible {
        return Err(Error::IncompatibleStack {
            path: path.into(),
            reason: match d.kind {
                DialogKind::AstmLink => {
                    "ASTM link requires serial/TCP, LIS01 framing and LIS02 syntax"
                }
                DialogKind::Hl7Receiver | DialogKind::LawManager => {
                    "HL7/LAW requires TCP, MLLP framing and HL7 syntax"
                }
                DialogKind::Poct => "POCT requires TCP, POCT framing and POCT syntax",
                DialogKind::VidasLiteral => {
                    "VIDAS requires serial/TCP, Literal framing and Literal syntax"
                }
                DialogKind::Autolyser => {
                    "Autolyser requires serial/TCP, Autolyser framing and syntax"
                }
                DialogKind::FilePickup => {
                    "file pickup requires watched_folder, no framing, and ASTM or HL7 syntax"
                }
            },
        });
    }
    let protocol_matches = match m.protocol {
        Protocol::Astm => m.syntax.kind == SyntaxKind::AstmLis02,
        Protocol::Hl7 => {
            m.syntax.kind == SyntaxKind::Hl7V2 && m.dialog.kind != DialogKind::LawManager
        }
        Protocol::Law => m.dialog.kind == DialogKind::LawManager,
        Protocol::Poct => m.syntax.kind == SyntaxKind::Poct,
        Protocol::Vidas => m.syntax.kind == SyntaxKind::VidasLiteral,
        Protocol::Autolyser => m.syntax.kind == SyntaxKind::Autolyser,
    };
    if !protocol_matches {
        return Err(invalid(
            path,
            "protocol",
            "selected protocol does not match syntax and dialog",
        ));
    }
    if (d.kind == DialogKind::AstmLink) != d.contention_priority.is_some()
        || matches!(d.kind, DialogKind::Hl7Receiver | DialogKind::LawManager)
            != d.acknowledgment.is_some()
    {
        return Err(invalid(
            path,
            "dialog",
            "declare ASTM contention priority or HL7 store_and_forward acknowledgment only for that dialog",
        ));
    }
    for (key, value) in [
        ("dialog.receive_timeout_ms", d.receive_timeout_ms),
        ("dialog.send_timeout_ms", d.send_timeout_ms),
    ] {
        if !(1..=300_000).contains(&value) {
            return Err(invalid(
                path,
                key,
                "use a deadline from 1 through 300000 milliseconds",
            ));
        }
    }
    if !(1..=16_777_216).contains(&f.limit) {
        return Err(invalid(
            path,
            "framing.limit",
            "use 1 through 16777216 bytes",
        ));
    }
    if (f.kind == FramingKind::AstmLis01) != f.checksum_hex.is_some() {
        return Err(invalid(
            path,
            "framing.checksum_hex",
            "declare checksum hex policy only for LIS01",
        ));
    }
    if f.kind == FramingKind::AstmLis01 {
        analyte_framing::astm::Limit::new(f.limit).map_err(|_| {
            invalid(
                path,
                "framing.limit",
                "LIS01 requires 8 through 64000 total bytes",
            )
        })?;
    }
    if (s.kind == SyntaxKind::Hl7V2)
        != (s.edition.is_some() && s.terminator.is_some() && s.escapes.is_some())
        || (s.kind == SyntaxKind::AstmLis02
            && (s.edition.is_some() || s.terminator.is_some() || s.escapes.is_some()))
    {
        return Err(invalid(
            path,
            "syntax",
            "provide edition, terminator and escapes together for HL7 only",
        ));
    }
    for (key, value, max) in [
        ("syntax.bytes", s.bytes, 16_777_216),
        ("syntax.records", s.records, 65536),
        ("syntax.record_bytes", s.record_bytes, 16_777_216),
        ("syntax.fields", s.fields, 1024),
        ("syntax.repetitions", s.repetitions, 1024),
        ("syntax.components", s.components, 1024),
    ] {
        if value == 0 || value > max {
            return Err(invalid(
                path,
                key,
                "use a positive value within the documented resource ceiling",
            ));
        }
    }
    // Compatibility choices must remain visible to reviewers as named departures.
    if s.allow_utf8_in_v231
        && (s.kind != SyntaxKind::Hl7V2
            || s.edition != Some(Edition::V231)
            || s.encoding != Encoding::Utf8)
    {
        return Err(invalid(
            path,
            "syntax.allow_utf8_in_v231",
            "use this departure only for HL7 2.3.1 with UTF-8 encoding",
        ));
    }
    let departures = [
        ("syntax.allow_utf8_in_v231", s.allow_utf8_in_v231),
        (
            "framing.limit",
            f.kind == FramingKind::AstmLis01 && f.limit != 64_000,
        ),
        (
            "framing.checksum_hex",
            matches!(f.checksum_hex, Some(ChecksumHex::AcceptLowercase)),
        ),
        (
            "dialog.receive_timeout_ms",
            d.kind == DialogKind::AstmLink && d.receive_timeout_ms != 30_000,
        ),
        (
            "dialog.send_timeout_ms",
            d.kind == DialogKind::AstmLink && d.send_timeout_ms != 15_000,
        ),
        (
            "dialog.contention_priority",
            matches!(d.contention_priority, Some(Priority::Host)),
        ),
        (
            "syntax.terminator",
            matches!(s.terminator, Some(Terminator::CrLf | Terminator::Lf)),
        ),
        (
            "syntax.escapes",
            matches!(s.escapes, Some(Escapes::Preserve)),
        ),
    ];
    let mut names = std::collections::HashSet::new();
    for deviation in &m.deviations {
        if !names.insert(&deviation.name) {
            return Err(invalid(
                path,
                "deviations.name",
                "use a unique deviation name",
            ));
        }
        if !departures
            .iter()
            .any(|(key, _)| *key == deviation.key.as_str())
        {
            return Err(invalid(
                path,
                "deviations.key",
                "name a supported compatibility option; arbitrary behavior cannot be expressed as a deviation",
            ));
        }
    }
    for (key, selected) in departures {
        if selected && !m.deviations.iter().any(|d| d.key.as_str() == key) {
            return Err(invalid(
                path,
                key,
                "add a named deviation with this key and a short reason for departing from the standard",
            ));
        }
    }
    validate_protocol(m, path)?;
    validate_mappings(m, path)?;
    if m.deviations.len() > 128 || m.evidence.capabilities.len() > 128 {
        return Err(invalid(
            path,
            "evidence/deviations",
            "limit each list to 128 entries",
        ));
    }
    for capability in &m.evidence.capabilities {
        for list in [
            &capability.dialogs,
            &capability.failure_cases,
            &capability.evidence_files,
        ] {
            if list.len() > 128 {
                return Err(invalid(
                    path,
                    "evidence.capabilities",
                    "limit capability lists to 128 entries",
                ));
            }
        }
        if m.evidence.level != EvidenceLevel::Documented && capability.evidence_files.is_empty() {
            return Err(invalid(
                path,
                "evidence.capabilities.evidence_files",
                "simulated or proven claims require evidence file references",
            ));
        }
    }
    if m.evidence.capabilities.is_empty() {
        return Err(invalid(
            path,
            "evidence.capabilities",
            "declare at least one capability, including untested scope",
        ));
    }
    Ok(())
}

fn validate_mappings(m: &MachineDocument, path: &str) -> Result<(), Error> {
    let Some(mapping) = &m.mappings else {
        let codes = m
            .codes
            .as_ref()
            .ok_or_else(|| invalid(path, "codes", "vendor syntax requires code tables"))?;
        return validate_tables(&codes.tests, &codes.units, path);
    };
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

fn validate_tables(tests: &[TestMapping], units: &[UnitMapping], path: &str) -> Result<(), Error> {
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

fn validate_protocol(m: &MachineDocument, path: &str) -> Result<(), Error> {
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
    let valid_ms = |value| (1..=300_000).contains(&value);
    let bad = |protocol| Error::ProtocolSettings {
        path: path.into(),
        protocol,
    };
    match (m.syntax.kind, &m.poct) {
        (SyntaxKind::Poct, Some(_))
        | (
            SyntaxKind::AstmLis02
            | SyntaxKind::Hl7V2
            | SyntaxKind::VidasLiteral
            | SyntaxKind::Autolyser,
            None,
        ) => {}
        (SyntaxKind::Poct, None)
        | (
            SyntaxKind::AstmLis02
            | SyntaxKind::Hl7V2
            | SyntaxKind::VidasLiteral
            | SyntaxKind::Autolyser,
            Some(_),
        ) => return Err(bad("poct")),
    }
    match (m.syntax.kind, &m.vidas) {
        (SyntaxKind::VidasLiteral, Some(_))
        | (
            SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2 | SyntaxKind::Poct | SyntaxKind::Autolyser,
            None,
        ) => {}
        (SyntaxKind::VidasLiteral, None)
        | (
            SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2 | SyntaxKind::Poct | SyntaxKind::Autolyser,
            Some(_),
        ) => return Err(bad("vidas")),
    }
    match (m.syntax.kind, &m.autolyser) {
        (SyntaxKind::Autolyser, Some(_))
        | (
            SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2 | SyntaxKind::Poct | SyntaxKind::VidasLiteral,
            None,
        ) => {}
        (SyntaxKind::Autolyser, None)
        | (
            SyntaxKind::AstmLis02 | SyntaxKind::Hl7V2 | SyntaxKind::Poct | SyntaxKind::VidasLiteral,
            Some(_),
        ) => return Err(bad("autolyser")),
    }
    if let Some(p) = &m.poct
        && (!(1..=128).contains(&p.pending_messages)
            || !(1..=64).contains(&p.depth)
            || !(1..=65536).contains(&p.elements)
            || !(1..=1048576).contains(&p.attribute_bytes)
            || !valid_ms(p.response_ms)
            || !valid_ms(p.acknowledgment_ms)
            || m.syntax.encoding != Encoding::Utf8)
    {
        return Err(bad("poct"));
    }
    if let Some(v) = &m.vidas
        && (!(1..=80).contains(&v.chunk)
            || m.framing.limit > 1920
            || !matches!(v.suffix.as_slice(), [] | [13] | [10] | [13, 10])
            || ![
                v.acknowledgment_ms,
                v.interbyte_ms,
                v.session_ms,
                v.commit_ms,
                v.backoff_ms,
            ]
            .into_iter()
            .all(valid_ms)
            || m.syntax.encoding != Encoding::Ascii)
    {
        return Err(bad("vidas"));
    }
    if let Some(a) = &m.autolyser
        && (!(2..=11038).contains(&m.framing.limit)
            || ![
                a.poll_ms,
                a.acknowledgment_ms,
                a.response_ms,
                a.interbyte_ms,
                a.storage_ms,
            ]
            .into_iter()
            .all(valid_ms)
            || m.syntax.encoding != Encoding::Ascii)
    {
        return Err(bad("autolyser"));
    }
    Ok(())
}
