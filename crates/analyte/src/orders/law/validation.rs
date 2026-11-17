//! Validates LAW work replies against retained orders (TF-2b B.3.1 and B.7.6).
use crate::{deadline, installation::config::Machine};
use ValidationError as Error;
use analyte_hl7::{Message, Options, law};
use analyte_store::Handle;

pub(crate) use law::Mismatch;

pub(crate) async fn check(
    store: &Handle,
    machine: &Machine,
    message: &Message,
    options: Options,
) -> Result<Option<Mismatch>, Error> {
    for returned in law::returned_work(message)? {
        let Some(placer) = returned.placer() else {
            continue;
        };
        let placer = std::str::from_utf8(placer)?.to_owned();
        let target = machine.id.clone();
        let lookup = placer.clone();
        let record = store
            .request::<_, analyte_store::OrderError>(deadline()?, move |s| {
                match s.machine_order_by_placer(&target, &lookup) {
                    Err(analyte_store::OrderError::Conflict) => {
                        Ok::<_, analyte_store::OrderError>(None)
                    }
                    other => Ok(other?),
                }
            })
            .await?;
        let unknown = returned.unknown();
        let Some(record) = record else {
            return Ok(Some(unknown));
        };
        let Some(item) = record
            .items
            .iter()
            .find(|item| item.placer.as_str() == placer)
        else {
            return Ok(Some(unknown));
        };
        let Some(bytes) = record.original_command() else {
            return Ok(Some(unknown));
        };
        let original = law::parse_message(bytes, options)?;
        if let Some(mismatch) =
            law::compare_retained(&original, item.placer.as_str(), &returned, unknown)
        {
            return Ok(Some(mismatch));
        }
    }
    Ok(None)
}

/// Operational failures comparing returned LAW work with retained immutable commands.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// A source occurrence counter exceeded its bound.
    #[error("SPM occurrence overflow")]
    SpmOccurrenceOverflow,
    /// A source occurrence counter exceeded its bound.
    #[error("SAC occurrence overflow")]
    SacOccurrenceOverflow,
    /// A source occurrence counter exceeded its bound.
    #[error("OBR occurrence overflow")]
    ObrOccurrenceOverflow,
    /// Preserves the original comparison failure.
    #[error(transparent)]
    Store(#[from] analyte_store::OrderError),
    /// The host could not represent the storage request deadline.
    #[error(transparent)]
    Deadline(#[from] crate::DeadlineError),
    /// Preserves the original comparison failure.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// Preserves the original comparison failure.
    #[error(transparent)]
    Hl7(#[from] analyte_hl7::Error),
}
