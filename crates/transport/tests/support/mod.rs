use std::{error::Error, io};
pub(crate) type Record = (u8, u128, Vec<u8>);
pub(crate) fn decode(mut input: &[u8]) -> Result<Vec<Record>, Box<dyn Error>> {
    let mut records = Vec::new();
    while !input.is_empty() {
        let mut magic = [0; 4];
        io::Read::read_exact(&mut input, &mut magic)?;
        assert_eq!(&magic, b"ATR1");
        let mut kind = [0];
        io::Read::read_exact(&mut input, &mut kind)?;
        let mut time = [0; 16];
        io::Read::read_exact(&mut input, &mut time)?;
        let mut length = [0; 4];
        io::Read::read_exact(&mut input, &mut length)?;
        let length = usize::try_from(u32::from_le_bytes(length))?;
        assert!(length <= analyte_transport::CHUNK_LIMIT);
        let mut bytes = vec![0; length];
        io::Read::read_exact(&mut input, &mut bytes)?;
        let tag = kind.first().copied().ok_or("missing tag")?;
        records.push((tag, u128::from_le_bytes(time), bytes));
    }
    Ok(records)
}
