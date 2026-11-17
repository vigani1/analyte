//! Writes outbound LAW query responses, work orders, rejections and result acknowledgments.
use crate::Message;
use crate::encode::wire::Wire;
use crate::law::{Error, raw};
mod query;
mod rejection;
mod result;
mod work;
pub use query::{CacheAnswer, QueryFollowup, QueryResponse, query_response};
pub use rejection::reject_trigger;
pub use result::result_acknowledgment;
pub use work::{Address, negative_work, work_order, work_order_with_profile};

// The generic ACK API accepts a scalar HL7 code and an unnamespaced local
// diagnostic. Keep the latter as text rather than inventing its coding system.
fn append_ack_segments(wire: &mut Wire, ack: &Message) -> Result<(), Error> {
    for segment in ack.segments().skip(1) {
        if segment.id() == b"ERR" {
            let mut code = raw(segment, 3).to_vec();
            code.push(ack.delimiters().component);
            code.push(ack.delimiters().component);
            code.extend_from_slice(b"HL70357");
            wire.append(b"ERR")?;
            for value in [
                b"".as_slice(),
                raw(segment, 2),
                &code,
                raw(segment, 4),
                b"",
                b"",
                b"",
                raw(segment, 5),
            ] {
                wire.field(value)?;
            }
        } else {
            wire.append(segment.raw())?;
        }
        wire.append(b"\r")?;
    }
    Ok(())
}
