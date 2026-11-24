//! Checks protocol stack settings and supported combinations.
use super::{Encoding, Error, TransportKind, invalid, validate_mapping_family};
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
    if t.kind != TransportKind::Serial && t.serial.is_some() {
        return Err(invalid(
            path,
            "transport.serial",
            "provide serial settings only for serial transport",
        ));
    }
    if let Some(s) = &t.serial {
        check_serial(s, path, "transport.serial")?;
    }
    Ok(())
}

/// Refuses a serial value outside the standard list; `table` names where the
/// values were written (`transport.serial` or the site's `serial`). An absent
/// value is checked when the layers are complete, never assumed.
pub(crate) fn check_serial(
    values: &crate::SerialValues,
    path: &str,
    table: &str,
) -> Result<(), Error> {
    if let Some(baud) = values.baud
        && !analyte_transport::serial::BAUD_RATES.contains(&baud)
    {
        return Err(invalid(
            path,
            &format!("{table}.baud"),
            "use a standard baud rate: 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600 or 115200",
        ));
    }
    if let Some(bits) = values.data_bits
        && !(5..=8).contains(&bits)
    {
        return Err(invalid(
            path,
            &format!("{table}.data_bits"),
            "use 5 through 8 data bits",
        ));
    }
    if let Some(stops) = values.stop_bits
        && !(1..=2).contains(&stops)
    {
        return Err(invalid(
            path,
            &format!("{table}.stop_bits"),
            "use one or two stop bits",
        ));
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

pub(crate) fn validate_dialog_and_framing(
    m: &ParsedMachine<'_>,
    path: &str,
) -> Result<Option<analyte_framing::astm::Limit>, Error> {
    let d = &m.dialog;
    let f = &m.framing;
    if (m.choice().has_astm_link()) != d.contention_priority.is_some()
        || m.choice().has_hl7_acknowledgment() != d.acknowledgment.is_some()
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
    if (m.choice().has_astm_link()) != f.checksum_hex.is_some() {
        return Err(invalid(
            path,
            "framing.checksum_hex",
            "declare checksum hex policy only for LIS01",
        ));
    }
    m.choice()
        .has_astm_link()
        .then(|| {
            analyte_framing::astm::Limit::new(f.limit).map_err(|_| {
                invalid(
                    path,
                    "framing.limit",
                    "LIS01 requires 8 through 64000 total bytes",
                )
            })
        })
        .transpose()
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
    if (m.choice().has_hl7_records())
        != (s.edition.is_some() && s.terminator.is_some() && s.escapes.is_some())
        || (m.choice().has_astm_records()
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
            m.choice().has_vidas_assay_codes(),
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
            ]
            .into_iter()
            .all(valid_ms)
            || m.syntax.encoding != Encoding::Ascii)
    {
        return Err(bad("vidas"));
    }
    if let Some(vidas) = &m.vidas
        && (vidas.dilution_required_assays.len() > 100
            || vidas
                .dilution_required_assays
                .iter()
                .enumerate()
                .any(|(index, code)| {
                    code.as_str().len() > 4
                        || !code
                            .as_str()
                            .bytes()
                            .all(|b| (32..=126).contains(&b) && b != b'|')
                        || vidas
                            .dilution_required_assays
                            .get(..index)
                            .is_some_and(|earlier| earlier.contains(code))
                }))
    {
        return Err(invalid(
            path,
            "vidas.dilution_required_assays",
            "supply up to 100 distinct printable assay codes of at most four bytes, without a pipe",
        ));
    }
    if let Some(a) = &m.autolyser
        && (!matches!(a.poll_ms, crate::schema::PollInterval::Milliseconds(ms) if valid_ms(ms))
            || !(2..=analyte_framing::autolyser::MAX_ENVELOPE).contains(&m.framing.limit)
            || ![
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
