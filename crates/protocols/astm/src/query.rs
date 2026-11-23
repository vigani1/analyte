//! Reads specimen-only ASTM query intent (LIS02-A2 sections 6 and 11).
use crate::{Kind, Message, Query};
/// Supported query shape, with specimen spelling preserved.
#[derive(Debug, PartialEq, Eq)]
pub enum QueryIntent {
    /// No query record was present.
    Absent,
    /// Exact specimen selectors.
    Orders(Vec<String>),
    /// One query with vendor-defined coordinates and an optional reliable specimen ID.
    Structured(StructuredQuery),
    /// Cancel a pending query.
    Cancel,
    /// Query outside the specimen-only subset.
    Unsupported,
}
/// A selector contains escapes that cannot be decoded faithfully.
#[derive(Debug, thiserror::Error)]
#[error("ASTM specimen selector contains unsupported escapes")]
pub struct QuerySelectorError;
/// Reads only production specimen queries; never expands ALL or ranges.
///
/// # Errors
/// Returns an error when a specimen selector contains unsupported escapes.
pub fn query_intent(messages: &[Message]) -> Result<QueryIntent, QuerySelectorError> {
    read(messages, None)
}

/// Reads one structured inquiry according to the selected documented layout.
/// # Errors
/// Returns an error for unsupported selector escapes.
pub fn structured_query_intent(
    messages: &[Message],
    profile: OrderProfile,
) -> Result<QueryIntent, QuerySelectorError> {
    read(messages, Some(profile))
}

fn read(
    messages: &[Message],
    profile: Option<OrderProfile>,
) -> Result<QueryIntent, QuerySelectorError> {
    let queries: Vec<_> = messages
        .iter()
        .flat_map(Message::records)
        .filter_map(Query::new)
        .collect();
    if queries.is_empty() {
        return Ok(QueryIntent::Absent);
    }
    // LIS02-A2 section 6.12 p.12: only production may reach the real order cache.
    // section 12.3 pp.27-28 permits a query error, never a fabricated empty answer.
    if messages
        .iter()
        .flat_map(Message::records)
        .filter_map(crate::Header::new)
        .any(|header| header.processing_id().is_none_or(|id| id.raw() != b"P"))
    {
        return Ok(QueryIntent::Unsupported);
    }
    if profile.is_some() && messages.iter().any(|m| m.delimiters().bytes() != *b"|\\^&") {
        return Ok(QueryIntent::Unsupported);
    }
    if queries.len() != 1
        || messages
            .iter()
            .flat_map(Message::records)
            .any(|r| !matches!(r.kind(), Kind::Header | Kind::Query | Kind::Terminator))
    {
        return Ok(QueryIntent::Unsupported);
    }
    let Some(query) = queries.first() else {
        return Ok(QueryIntent::Unsupported);
    };
    if query.field(13).is_some_and(|f| f.raw() == b"A") {
        return Ok(QueryIntent::Cancel);
    }
    if query.field(13).is_none_or(|f| f.raw() != b"O")
        || (4..=12)
            .filter(|f| *f != 5)
            .any(|f| query.field(f).is_some_and(|f| !f.raw().is_empty()))
        || query
            .test_id()
            .is_some_and(|f| !matches!(f.raw(), b"" | b"ALL"))
    {
        return Ok(QueryIntent::Unsupported);
    }
    let Some(ids) = query.range_start() else {
        return Ok(QueryIntent::Unsupported);
    };
    if let Some(profile) = profile {
        return Ok(
            structured(ids, profile).map_or(QueryIntent::Unsupported, QueryIntent::Structured)
        );
    }
    let mut samples = Vec::new();
    for repetition in ids.repeats() {
        if repetition
            .iter()
            .enumerate()
            .any(|(index, part)| index != 1 && !part.raw().is_empty())
        {
            return Ok(QueryIntent::Unsupported);
        }
        let Some(sample) = repetition.get(1) else {
            return Ok(QueryIntent::Unsupported);
        };
        if sample.raw() == b"ALL" || sample.raw().is_empty() {
            return Ok(QueryIntent::Unsupported);
        }
        if sample.has_unsupported_escape() {
            return Err(QuerySelectorError);
        }
        samples.push(sample.text().to_owned());
    }
    if samples.is_empty() {
        return Ok(QueryIntent::Unsupported);
    }
    Ok(QueryIntent::Orders(samples))
}

/// Structured ASTM order layout from HIM v2.3 Tables 11-17.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderProfile {
    /// Elecsys layout.
    Elecsys,
    /// cobas layout.
    Cobas,
}

/// One query selector and the exact instrument coordinates it supplied.
#[derive(Debug, PartialEq, Eq)]
pub struct StructuredQuery {
    sample: String,
    parts: [String; 6],
}

impl StructuredQuery {
    /// Returns a barcode eligible for lookup; generated and absent IDs have none.
    #[must_use]
    pub fn sample(&self) -> Option<&str> {
        (!self.sample.is_empty() && !self.sample.starts_with('@')).then_some(&self.sample)
    }

    /// Returns the six O.4 components in order, keeping their original spelling.
    #[must_use]
    pub fn instrument_id(&self) -> &[String; 6] {
        &self.parts
    }
}

fn structured(field: &crate::Field, profile: OrderProfile) -> Option<StructuredQuery> {
    let [parts] = field.repeats() else {
        return None;
    };
    let offset = match profile {
        OrderProfile::Elecsys => 1,
        OrderProfile::Cobas => 2,
    };
    let (reserved, values) = parts.split_at_checked(offset)?;
    if reserved.iter().any(|c| !c.raw().is_empty()) {
        return None;
    }
    let [sample, sequence, carrier, slot, blank, specimen, container] = values else {
        return None;
    };
    if values.iter().any(|c| {
        c.raw() != c.text().as_bytes()
            || c.raw()
                .iter()
                .any(|b| !(32..127).contains(b) || b"|\\^&".contains(b))
    }) || sample.raw().len() > 22
        || sample.raw() == b"ALL"
        || !numeric(sequence.raw(), 4)
        || !numeric(slot.raw(), 2)
        || carrier.raw().is_empty()
        || carrier.raw().len() > 5
        || !blank.raw().is_empty()
    {
        return None;
    }
    let types = match profile {
        OrderProfile::Elecsys => {
            specimen.raw() == b"SAMPLE" && matches!(container.raw(), b"NORMAL" | b"REDUCED")
        }
        OrderProfile::Cobas => {
            matches!(specimen.raw(), b"S0" | b"S1" | b"S2" | b"S5")
                && matches!(container.raw(), b"SC" | b"MC")
        }
    };
    types.then(|| StructuredQuery {
        sample: sample.text().to_owned(),
        parts: [sequence, carrier, slot, blank, specimen, container].map(|c| c.text().to_owned()),
    })
}

fn numeric(value: &[u8], max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.iter().all(u8::is_ascii_digit)
}

/// Recognizes the query record prefix for starting the host's deadline.
/// This hint does not validate query intent or authorize a cache answer.
#[must_use]
pub fn starts_query(record: &[u8]) -> bool {
    record.first() == Some(&b'Q')
}

#[cfg(test)]
mod tests {
    use super::*;
    // Roche cobas e 411 HIM v2.3 Table 14 and section 17 (pp. 48-50).
    #[test]
    fn structured_selectors_preserve_coordinates_and_exclude_generated_ids()
    -> Result<(), crate::Error> {
        for (profile, prefix, specimen, container) in [
            (OrderProfile::Elecsys, "^", "SAMPLE", "REDUCED"),
            (OrderProfile::Cobas, "^^", "S0", "MC"),
        ] {
            for (sample, status) in [("001z", "O"), ("", "O"), ("@73", "O"), ("001z", "A")] {
                let payload = format!(
                    "H|\\^&||||||||||P||\rQ|1|{prefix}{sample}^0073^@031^02^^{specimen}^{container}||ALL||||||||{status}\rL|1|N\r"
                );
                let parsed = crate::parse(payload.as_bytes(), crate::Limits::default())?;
                let intent = structured_query_intent(&parsed, profile).unwrap();
                if status == "A" {
                    assert_eq!(intent, QueryIntent::Cancel);
                } else {
                    let QueryIntent::Structured(query) = intent else {
                        panic!("expected structured query")
                    };
                    assert_eq!(query.sample(), (sample == "001z").then_some(sample));
                    assert_eq!(
                        query.instrument_id(),
                        &["0073", "@031", "02", "", specimen, container]
                    );
                    assert_eq!(query_intent(&parsed).unwrap(), QueryIntent::Unsupported);
                }
            }
        }
        Ok(())
    }

    // CLSI LIS02-A2 §5.4, §11
    #[test]
    fn specimen_queries_keep_escapes_refuse_unknowns() -> Result<(), crate::Error> {
        for (selector, expected) in [
            ("001a", "001a"),
            ("A&F&B", "A|B"),
            ("A&S&B", "A^B"),
            ("A&R&B", "A\\B"),
            ("A&E&B", "A&B"),
        ] {
            let payload = format!("H|\\^&||||||||||P|LIS2-A2\rQ|1|^{selector}||||||||||O\rL|1|N\r");
            let messages = crate::parse(payload.as_bytes(), crate::Limits::default())?;
            assert_eq!(
                query_intent(&messages).expect("supported escapes"),
                QueryIntent::Orders(vec![expected.to_owned()])
            );
        }
        for (status, processing, selector, expected) in [
            ("A", "P", "001a", QueryIntent::Cancel),
            ("O", "T", "001a", QueryIntent::Unsupported),
            ("O", "P", "ALL", QueryIntent::Unsupported),
        ] {
            let payload = format!(
                "H|\\^&||||||||||{processing}|LIS2-A2\rQ|1|^{selector}||||||||||{status}\rL|1|N\r"
            );
            let messages = crate::parse(payload.as_bytes(), crate::Limits::default())?;
            assert_eq!(query_intent(&messages).expect("plain selector"), expected);
        }
        Ok(())
    }
}

// CLSI LIS02-A2 §11
#[test]
fn query_prefix_starts_budget_without_validating() {
    for raw in [b"Q|1|^001\r".as_slice(), b"Q", b"Q|malformed"] {
        assert!(starts_query(raw));
    }
    for raw in [b"".as_slice(), b"H|x\r", b"q|1\r"] {
        assert!(!starts_query(raw));
    }
}
