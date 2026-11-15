//! Private bounded wire assembly; callers retain their own error categories.
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

    pub(crate) fn field(&mut self, value: &[u8]) -> Result<(), Limit> {
        self.append(&[self.separator])?;
        self.append(value)
    }
}
