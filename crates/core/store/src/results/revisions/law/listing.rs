//! Lists protected LAW execution conditions and receipt-linked interpretation history.
use super::*;
use crate::{MachineOrder, MachineOrderItem, Store};

/// One actionable condition per AWOS; later CA receipts keep the same pagination identity.
pub struct AnalyzerCancelledItem {
    /// Stable first cancellation sequence.
    pub cursor: i64,
    /// Retained order and reserved app version.
    pub order: MachineOrder,
    /// Exact cancelled item, including latest receipt evidence.
    pub item: MachineOrderItem,
}
/// Protected source evidence for one retained AWOS.
pub struct LawWorkEvidence {
    /// Stable pagination sequence from the physical receipt and interpretation.
    pub cursor: i64,
    /// Physical receipt identity.
    pub receipt: analyte_model::ReceiptId,
    /// Interpretation version.
    pub version: i64,
    /// Time received by the host.
    pub received_at: SourceText,
    /// Supplied interpretation and commit-order evidence.
    pub custody: LawCustody,
    /// Committed whole-message outcome.
    pub disposition: LawDisposition,
}

impl Store {
    /// Reads a retained order directly, without consulting the mutable app cache.
    /// # Errors
    /// Reports missing, corrupt or purged retained work.
    pub fn machine_order_by_id(&self, id: i64) -> Result<MachineOrder, Error> {
        crate::machine_orders::load(&self.db, id)
    }

    /// Counts distinct analyzer-cancelled AWOSs without exposing source text.
    /// # Errors
    /// Reports database failures or invalid retained counts.
    pub fn analyzer_cancelled_count(&self) -> Result<u64, Error> {
        let count:i64=self.db.query_row("SELECT count(*) FROM (SELECT machine_order,item_index FROM awos_execution_history WHERE state='AnalyzerCancelled' GROUP BY machine_order,item_index)",[],|r|r.get(0))?;
        u64::try_from(count).map_err(|_| Error::Corruption)
    }

    /// Returns one bounded page of stable analyzer cancellation conditions.
    /// # Errors
    /// Reports invalid retained work or database failures.
    pub fn analyzer_cancelled_items(
        &self,
        after: i64,
    ) -> Result<Vec<AnalyzerCancelledItem>, Error> {
        let mut query=self.db.prepare("SELECT min(sequence),machine_order,item_index FROM awos_execution_history WHERE state='AnalyzerCancelled' GROUP BY machine_order,item_index HAVING min(sequence)>?1 ORDER BY min(sequence) LIMIT ?2")?;
        let rows = query
            .query_map(
                params![
                    after,
                    i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, u16>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(cursor, id, index)| {
                let order = self.machine_order_by_id(id)?;
                let item = order
                    .items
                    .iter()
                    .find(|i| i.index == index)
                    .cloned()
                    .ok_or(Error::Corruption)?;
                Ok(AnalyzerCancelledItem {
                    cursor,
                    order,
                    item,
                })
            })
            .collect()
    }

    /// Returns accepted and rejected message evidence naming this retained AWOS.
    /// A rejected proposal is history, never proof that its projection was applied.
    /// # Errors
    /// Reports bounds, corrupt evidence or database failures.
    pub fn law_work_evidence(
        &self,
        order: i64,
        item: u16,
        after: i64,
    ) -> Result<Vec<LawWorkEvidence>, Error> {
        let mut query=self.db.prepare("SELECT m.rowid,m.receipt,m.version,m.evidence,m.disposition,r.metadata FROM law_messages m JOIN receipts r ON r.id=m.receipt WHERE m.rowid>?3 AND EXISTS(SELECT 1 FROM json_each(m.evidence,'$.work') w WHERE json_extract(w.value,'$.order')=?1 AND json_extract(w.value,'$.item')=?2) ORDER BY m.rowid LIMIT ?4")?;
        let rows = query.query_map(
            params![
                order,
                item,
                after,
                i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidInput)?
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )?;
        rows.map(|r| {
            let (cursor, receipt, version, evidence, disposition, metadata) = r?;
            if evidence.len() > self.limits.record_bytes
                || metadata.len() > self.limits.record_bytes
            {
                return Err(Error::Corruption);
            }
            let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
            Ok(LawWorkEvidence {
                cursor,
                receipt: serde_json::from_str(&receipt)?,
                version,
                received_at: SourceText::new(&metadata.received_at.to_string())
                    .map_err(|_| Error::Corruption)?,
                custody: serde_json::from_str(&evidence)?,
                disposition: serde_json::from_str(&disposition)?,
            })
        })
        .collect()
    }
}
