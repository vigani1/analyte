//! Writes HL7 segments into a size-capped byte buffer without escaping, shared by LAW renderers.
#[derive(Debug)]
pub(crate) struct Limit;

pub(crate) fn append(output: &mut Vec<u8>, value: &[u8], maximum: usize) -> Result<(), Limit> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|size| size > maximum)
    {
        return Err(Limit);
    }
    output.extend_from_slice(value);
    Ok(())
}

/// Bounded output assembly shared by LAW renderers; check before allocating.
pub(crate) struct Wire {
    pub(crate) bytes: Vec<u8>,
    maximum: usize,
    separator: u8,
}

impl Wire {
    pub(crate) fn new(maximum: usize, separator: u8) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            separator,
        }
    }

    pub(crate) fn append(&mut self, value: &[u8]) -> Result<(), Limit> {
        append(&mut self.bytes, value, self.maximum)
    }

    pub(crate) fn segment(&mut self, name: &[u8], fields: &[&[u8]]) -> Result<(), Limit> {
        segment(&mut self.bytes, name, fields, self.maximum, self.separator)
    }
    pub(crate) fn field(&mut self, value: &[u8]) -> Result<(), Limit> {
        self.append(&[self.separator])?;
        self.append(value)
    }
}

pub(crate) fn segment(
    output: &mut Vec<u8>,
    name: &[u8],
    fields: &[&[u8]],
    maximum: usize,
    separator: u8,
) -> Result<(), Limit> {
    append(output, name, maximum)?;
    for value in fields {
        append(output, &[separator], maximum)?;
        append(output, value, maximum)?;
    }
    append(output, b"\r", maximum)
}
