//! Reads specimen-only ASTM query intent (LIS02-A2 sections 6 and 11).
use crate::{Kind, Message, Query};
/// Supported query shape, with specimen spelling preserved.
#[derive(Debug, PartialEq, Eq)]
pub enum QueryIntent {
    /// No query record was present.
    Absent,
    /// Exact specimen selectors.
    Orders(Vec<String>),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lis02_a2_sections_5_4_11_specimen_queries_preserve_escapes_and_refuse_unknowns()
    -> Result<(), crate::Error> {
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
