//! Checks protocol stack settings and supported combinations.
use super::*;

pub(super) fn validate_transport(m: &MachineDocument, path: &str) -> Result<(), Error> {
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
    Ok(())
}

pub(super) fn validate_stack(m: &MachineDocument, path: &str) -> Result<(), Error> {
    let t = &m.transport;
    let f = &m.framing;
    let d = &m.dialog;
    if let Some(enabled) = d.enhanced_application_acknowledgments
        && (!m
            .syntax
            .edition
            .is_some_and(|edition| analyte_hl7::Version::from(edition).paired_application_ack())
            || enabled)
    {
        return Err(invalid(
            path,
            "dialog.enhanced_application_acknowledgments",
            "HL7 2.9.1 enhanced application acknowledgments require an unsupported separate channel; only false is supported for v291",
        ));
    }
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
    Ok(())
}

pub(super) fn validate_protocol_selection(m: &MachineDocument, path: &str) -> Result<(), Error> {
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
    Ok(())
}

pub(super) fn validate_dialog_and_framing(m: &MachineDocument, path: &str) -> Result<(), Error> {
    let d = &m.dialog;
    let f = &m.framing;
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
    Ok(())
}

pub(super) fn validate_syntax(m: &MachineDocument, path: &str) -> Result<(), Error> {
    let s = &m.syntax;
    if let Some(edition) = s.edition.map(analyte_hl7::Version::from)
        && !edition.supports_result_reception()
    {
        return Err(Error::UnsupportedHl7Edition {
            path: path.into(),
            edition: edition.as_str(),
        });
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
    Ok(())
}

pub(super) fn validate_protocol(m: &MachineDocument, path: &str) -> Result<(), Error> {
    validate_mapping_family(m, path)?;
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
