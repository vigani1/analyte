//! Restarts exhausted notification attempts with an explicit audited operator decision.
use crate::operator::Error;
use crate::{deadline, installation::config::Program};
use analyte_model::{Identifier, Reason};

/// Retries retained exhausted alerts for a configured endpoint, preserving all bytes.
/// # Errors
/// Refuses unknown configuration, missing exhausted work or storage/audit failures.
pub async fn retry(
    program: &Program,
    actor: Identifier,
    endpoint: Identifier,
    reason: Reason,
) -> Result<(), Error> {
    if !program
        .alert_delivery()
        .endpoints()
        .iter()
        .any(|entry| entry.id() == &endpoint)
    {
        return Err(Error::AlertRetryEndpointIsNotConfigured);
    }
    let audit = analyte_store::Audit {
        actor,
        action: Identifier::new("alert-retry")?,
        targets: vec![endpoint.clone()],
        time: jiff::Timestamp::now().as_second(),
        reason,
    };
    let mut store = crate::open_system_store(program).await?;
    let result = store
        .request(deadline()?, move |s| {
            Ok(s.retry_failed_alerts(&endpoint, &audit)?)
        })
        .await;
    store.shutdown(deadline()?).await?;
    result?;
    super::output::line("Exhausted alert events queued with original identities.")?;
    Ok(())
}
