//! Synthetic LAW program/reference-receiver matrix, TF-2b 3.29.4, B.3 and B.7.
use super::*;
use crate::support::HANG_GUARD;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn definition() -> String {
    include_str!("../../../../core/config/examples/law.toml").to_owned()
}

pub(super) fn result(id: &str, awos: &str, filler: &str, value: &str, status: &str) -> String {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, "NM"),
        (2, "SYN^Synthetic^99LAB"),
        (3, "1"),
        (4, value),
        (5, "mg/L^mg/L^UCUM"),
        (6, "0-20"),
        (7, "N^Normal^HL70078"),
        (10, status),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!(
        "MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|{id}|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||00012a^LAB\rOBR||{awos}|{filler}|SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|{}\r",
        fields.join("|")
    )
}

fn count(path: &Path, table: &str) -> Result<i64, Error> {
    // Callers supply only literal test-owned table names.
    Ok(database(path)?.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
}

fn check(
    event: &Captured,
    order: &str,
    namespace: &str,
    revision: u64,
    value: &str,
) -> Result<(), Error> {
    assert!(event.outcome.contains("accepted"));
    let body: serde_json::Value = serde_json::from_slice(&event.bytes)?;
    assert_eq!(
        body.pointer("/results/0/sample/id")
            .and_then(|v| v.as_str()),
        Some("00012a")
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/value")
            .and_then(|v| v.as_str()),
        Some(order)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/observation/order/namespace")
            .and_then(|v| v.as_str()),
        Some(namespace)
    );
    assert_eq!(
        body.pointer("/results/0/identity/resolved/revision")
            .and_then(serde_json::Value::as_u64),
        Some(revision)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/values/0/original")
            .and_then(|v| v.as_str()),
        Some(value)
    );
    assert_eq!(
        body.pointer("/results/0/parts/0/units/original")
            .and_then(|v| v.as_str()),
        Some("mg/L^mg/L^UCUM")
    );
    Ok(())
}

#[cfg(feature = "test-hooks")]
#[cfg(feature = "test-hooks")]
async fn read_reply(peer: &mut tokio::net::TcpStream) -> Result<String, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\x1c\r") {
            if bytes.len() >= 65536 {
                return Err("reply bound".into());
            }
            bytes.push(peer.read_u8().await?);
        }
        Ok::<_, Error>(String::from_utf8(bytes)?)
    })
    .await?
}

async fn assert_repaired_mapping_preserves_receipt(
    store: &mut analyte_store::Handle,
    current: &Program,
    id: analyte_model::ReceiptId,
) -> Result<(), Error> {
    let (parked, original, version1) = store
        .request(reprocess_deadline()?, move |s| {
            let parked = s
                .parked(0)?
                .first()
                .ok_or(analyte_store::Error::Corruption)?
                .id;
            let original = s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)?;
            let version1 = serde_json::to_string(
                &s.interpretation(id, 1)?
                    .ok_or(analyte_store::Error::Corruption)?,
            )?;
            Ok((parked, original, version1))
        })
        .await?;
    let commit = analyte::operator::recovery::reprocess(
        store,
        current,
        analyte::operator::recovery::ReprocessTarget::Parked(parked),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("repair LAW test mapping")?,
    )
    .await?;
    assert_eq!(commit.version, 2);
    assert!(matches!(
        commit.outcomes.as_slice(),
        [analyte_store::RevisionOutcome::New(_)]
    ));
    let repeated = analyte::operator::recovery::reprocess(
        store,
        current,
        analyte::operator::recovery::ReprocessTarget::Receipt(id),
        analyte_model::Identifier::new("synthetic-operator")?,
        analyte_model::Reason::new("verify retained result idempotency")?,
    )
    .await?;
    assert_eq!(repeated.version, 3);
    assert!(matches!(
        repeated.outcomes.as_slice(),
        [analyte_store::RevisionOutcome::Retransmission(_)]
    ));
    store
        .request(reprocess_deadline()?, move |s| {
            let retained = s.receipt(id)?.ok_or(analyte_store::Error::UnknownReceipt)?;
            assert_eq!(retained.raw, original.raw);
            assert_eq!(retained.config, original.config);
            assert_eq!(
                serde_json::to_string(&retained.metadata)?,
                serde_json::to_string(&original.metadata)?
            );
            assert_eq!(
                serde_json::to_string(
                    &s.interpretation(id, 1)?
                        .ok_or(analyte_store::Error::Corruption)?
                )?,
                version1
            );
            assert!(
                s.parked_item(parked)?
                    .ok_or(analyte_store::Error::Corruption)?
                    .resolution
                    .is_some()
            );
            assert_eq!(s.deliveries(0)?.len(), 1);
            assert_eq!(s.audits(0)?.len(), 2);
            Ok(())
        })
        .await?;
    store.shutdown(reprocess_deadline()?).await?;
    Ok(())
}

fn reprocess_deadline() -> Result<std::time::Instant, &'static str> {
    std::time::Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("deadline")
}

mod scenarios_1;

mod scenarios_2;
