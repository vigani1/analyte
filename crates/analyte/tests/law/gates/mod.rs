//! Exercises LAW host ordering with bounded gates and durable observations.
use super::*;
use analyte::runtime::test_hooks::{Committed, LawGates, StoreGate, StoreProbe};
use std::sync::Arc;

async fn start(
    settings: &str,
    orders: Vec<analyte_contract::Order>,
    gates: LawGates,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", settings)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    for batch in orders.chunks(100) {
        reference.receiver.seed(batch.to_vec()).await?;
    }
    let mut loaded = program(&path, &reference.url)?;
    loaded.install_law_gates_for_test("law", gates)?;
    loaded.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let peer = connect(&path, "law").await?;
    wait_feed(&path).await?;
    Ok((temp, path, reference, running, peer))
}
fn items(path: &Path) -> Result<Vec<(String, String)>, Error> {
    Ok(database(path)?
        .prepare("SELECT placer,state FROM machine_order_items ORDER BY item_index")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}
async fn no_dial(listener: &TcpListener) -> Result<(), Error> {
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err(),
        "outbound dial crossed gated ownership boundary"
    );
    Ok(())
}
fn independent(endpoint: std::net::SocketAddr) -> String {
    definition().rewrite(
        "connection_mode = \"persistent\"",
        &format!(
            "connection_mode = \"independent_per_transaction\"\noutbound_endpoint = \"{endpoint}\""
        ),
    )
}
async fn uncertain(path: &Path, expected: &[(String, String)]) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM machine_order_items WHERE state='Uncertain'",
        [],
        i64::try_from(expected.len())?,
    )
    .await?;
    assert_eq!(
        items(path)?,
        expected
            .iter()
            .map(|(id, _)| (id.clone(), "Uncertain".into()))
            .collect::<Vec<_>>()
    );
    Ok(())
}
mod handover;
mod lookup;
mod paging;
