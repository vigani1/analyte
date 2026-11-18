//! Builds and encodes bounded ASTM order records (LIS02-A2 section 8).
/// Specific refusal while constructing an outbound O record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OrderError {
    /// Text contains unsupported bytes or reserved separators.
    #[error("ASTM order text repertoire")]
    Text,
    /// Selected field is outside the order record.
    #[error("ASTM order selector field")]
    Field,
    /// Encoded record exceeds a configured byte bound.
    #[error("ASTM order byte limit")]
    Limit,
}

/// Typed order fields; construction validates the outbound repertoire and byte bounds.
#[derive(Debug, Clone)]
pub struct OrderRecord {
    fields: Vec<Vec<Vec<String>>>,
}

impl OrderRecord {
    /// Constructs a complete O record from repetition/component values.
    /// Fields start at O-3; the record kind and sequence are supplied by this grammar.
    /// # Errors
    /// Refuses unsupported text, field counts and encoded byte bounds.
    pub fn new(
        values: Vec<Vec<Vec<String>>>,
        record_limit: usize,
        message_limit: usize,
    ) -> Result<Self, OrderError> {
        if !matches!(values.len(), 10 | 14) {
            return Err(OrderError::Field);
        }
        for value in values.iter().flatten().flatten() {
            validate_text(value)?;
        }
        let mut fields = vec![vec![vec!["O".into()]], vec![vec!["1".into()]]];
        fields.extend(values);
        let record = Self { fields };
        let size = record
            .fields
            .iter()
            .try_fold(0_usize, |size, field| {
                field
                    .iter()
                    .try_fold(size, |size, repetition| {
                        repetition
                            .iter()
                            .try_fold(size, |size, part| size.checked_add(part.len()))
                            .and_then(|n| n.checked_add(repetition.len().saturating_sub(1)))
                    })
                    .and_then(|n| n.checked_add(field.len().saturating_sub(1)))
            })
            .and_then(|n| n.checked_add(record.fields.len().saturating_sub(1)))
            .ok_or(OrderError::Limit)?;
        if size > record_limit || size.checked_add(1).is_none_or(|n| n > message_limit) {
            return Err(OrderError::Limit);
        }
        Ok(record)
    }

    /// Encodes validated fields using the supported standard delimiters, including CR.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut text = self
            .fields
            .iter()
            .map(|field| {
                field
                    .iter()
                    .map(|parts| parts.join("^"))
                    .collect::<Vec<_>>()
                    .join("\\")
            })
            .collect::<Vec<_>>()
            .join("|");
        text.push('\r');
        text.into_bytes()
    }
}

fn validate_text(value: &str) -> Result<(), OrderError> {
    if value.len() > 4096
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b))
    {
        return Err(OrderError::Text);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // CLSI LIS02-A2 §8
    #[test]
    fn order_record_golden_bytes_and_bounds() {
        let mut fields = vec![Vec::new(); 10];
        fields[0] = vec![vec!["".into(), "".into(), "".into(), "001a".into()]];
        fields[2] = vec![vec!["".into(), "".into(), "".into(), "SEND".into()]];
        fields[3] = vec![vec!["R".into()]];
        fields[9] = vec![vec!["N".into()]];
        let expected = b"O|1|^^^001a||^^^SEND|R||||||N\r";
        assert_eq!(
            OrderRecord::new(fields.clone(), expected.len() - 1, expected.len())
                .unwrap()
                .encode(),
            expected
        );
        assert!(matches!(
            OrderRecord::new(fields.clone(), expected.len() - 2, expected.len()),
            Err(OrderError::Limit)
        ));
        assert!(matches!(
            OrderRecord::new(fields.clone(), expected.len(), expected.len() - 1),
            Err(OrderError::Limit)
        ));
        fields[0] = vec![vec!["bad|id".into()]];
        assert!(matches!(
            OrderRecord::new(fields, 1024, 1024),
            Err(OrderError::Text)
        ));
    }
}
