//! Recovers raw LAW receipts before opening any machine connection.
use crate::runtime::Error;
use crate::{deadline, installation::config::Program};
use analyte_store::{Handle, Interpretation, Reading};

pub(crate) async fn run(program: &Program, store: &Handle) -> Result<(), Error> {
    for machine in program.machines() {
        if !matches!(
            machine.snapshot.effective().stack(),
            analyte_config::Stack::Law(_)
        ) {
            continue;
        }
        let mut after = 0;
        let mut complete = false;
        for _ in 0..128 {
            let target = machine.id.clone();
            let pending = store
                .request(deadline()?, move |s| {
                    s.pending_interpretations(&target, after)
                })
                .await?;
            if pending.is_empty() {
                complete = true;
                break;
            }
            for (sequence, metadata) in pending {
                after = sequence;
                if metadata.config_digest.0 == machine.snapshot.digest() {
                    crate::intake::interpret_with_location(store, machine, metadata.id).await?;
                } else {
                    let id = metadata.id;
                    store.request(deadline()?,move |s| {
                        let receipt=s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)?;
                        let evidence=Interpretation {driver:metadata.driver,config_version:metadata.config_version,config:receipt.config,outcome:Reading::Failed(analyte_model::Reason::new("pending LAW receipt has a different retained configuration; explicit audited reprocessing required").map_err(|_|analyte_store::Error::InvalidHostInput)?),departures:None};
                        s.interpret_reconciled(id,evidence,&[],&[],|_,_|Ok(Vec::new()))
                    }).await?;
                }
            }
        }
        if !complete {
            return Err(Error::PendingLawRecoveryPageBoundExceededMachineStartupRefused);
        }
    }
    Ok(())
}
