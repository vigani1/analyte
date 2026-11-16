//! Transport test fixtures belong beside integration tests to share bounded synthetic peers.
use std::error::Error;
pub(crate) type Record = (u8, u128, Vec<u8>);
pub(crate) fn decode(input: &[u8]) -> Result<Vec<Record>, Box<dyn Error>> {
    use analyte_transport::recording::{Direction, Limits, Link, read};
    let (_, events) = read(input, Limits::default())?;
    Ok(events
        .into_iter()
        .map(|e| {
            let tag = match e.link {
                Some(Link::Connect) => 0,
                Some(Link::Disconnect) => 3,
                Some(Link::Break) => 4,
                None => match e.direction {
                    Direction::MachineToHost => 1,
                    Direction::HostToMachine => 2,
                },
            };
            (tag, u128::from(e.time_ms), e.bytes)
        })
        .collect())
}
