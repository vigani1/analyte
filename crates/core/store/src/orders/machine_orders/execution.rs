//! Reads the latest immutable AWOS projection without altering command lifecycle evidence.
use crate::Error;
use analyte_model::{AwosExecution, AwosExecutionProjection};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn load(
    db: &Connection,
    order: i64,
    index: u16,
) -> Result<Option<AwosExecutionProjection>, Error> {
    let row = db.query_row(
        "SELECT state,receipt,interpretation,recorded_at,cancel_outstanding FROM awos_execution_history WHERE machine_order=?1 AND item_index=?2 ORDER BY sequence DESC LIMIT 1",
        params![order,index],
        |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,u32>(2)?,row.get::<_,i64>(3)?,row.get::<_,bool>(4)?)),
    ).optional()?;
    row.map(
        |(state, receipt, interpretation, recorded_at, cancel_outstanding)| {
            let state = match state.as_str() {
                "Scheduled" => AwosExecution::Scheduled,
                "InProgress" => AwosExecution::InProgress,
                "AnalyzerCancelled" => AwosExecution::AnalyzerCancelled,
                _ => return Err(Error::Corruption),
            };
            Ok(AwosExecutionProjection {
                state,
                receipt: serde_json::from_str(&receipt)?,
                interpretation: std::num::NonZeroU32::new(interpretation)
                    .ok_or(Error::Corruption)?,
                recorded_at,
                cancel_outstanding,
            })
        },
    )
    .transpose()
}
