//! Checks protocol stack settings and supported combinations.
use super::*;
use crate::{ParsedMachine, StackChoice};

pub(super) fn validate_transport(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
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

pub(super) fn validate_stack(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    if let Some(enabled) = m.dialog.enhanced_application_acknowledgments
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
    Ok(())
}

pub(super) fn validate_dialog_and_framing(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    let d = &m.dialog;
    let f = &m.framing;
    if (matches!(m.choice(), StackChoice::AstmLis02)) != d.contention_priority.is_some()
        || matches!(m.choice(), StackChoice::Hl7V2 | StackChoice::LawManager)
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
    if (matches!(m.choice(), StackChoice::AstmLis02)) != f.checksum_hex.is_some() {
        return Err(invalid(
            path,
            "framing.checksum_hex",
            "declare checksum hex policy only for LIS01",
        ));
    }
    if matches!(m.choice(), StackChoice::AstmLis02) {
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

pub(super) fn validate_syntax(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    let s = &m.syntax;
    if let Some(edition) = s.edition.map(analyte_hl7::Version::from)
        && !edition.supports_result_reception()
    {
        return Err(Error::UnsupportedHl7Edition {
            path: path.into(),
            edition: edition.as_str(),
        });
    }
    if (matches!(
        m.choice(),
        StackChoice::Hl7V2 | StackChoice::FileHl7 | StackChoice::LawManager
    )) != (s.edition.is_some() && s.terminator.is_some() && s.escapes.is_some())
        || (matches!(m.choice(), StackChoice::AstmLis02 | StackChoice::FileAstm)
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

pub(super) fn validate_protocol(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    validate_mapping_family(m, path)?;
    let valid_ms = |value| (1..=300_000).contains(&value);
    let bad = |protocol| Error::ProtocolSettings {
        path: path.into(),
        protocol,
    };
    for (selected, supplied, protocol) in [
        (
            matches!(m.choice(), StackChoice::Poct),
            m.poct.is_some(),
            "poct",
        ),
        (
            matches!(m.choice(), StackChoice::VidasLiteral),
            m.vidas.is_some(),
            "vidas",
        ),
        (
            matches!(m.choice(), StackChoice::Autolyser),
            m.autolyser.is_some(),
            "autolyser",
        ),
    ] {
        if selected != supplied {
            return Err(bad(protocol));
        }
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
