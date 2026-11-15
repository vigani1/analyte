use super::*;
use std::{path::Path, time::Duration};

struct Space;
impl analyte_store::FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

fn machine(policy: &str) -> Result<Machine, Error> {
    let section = include_str!("../../../config/examples/orders.toml").replace(
        "when_untrusted = \"error\"",
        &format!("when_untrusted = \"{policy}\""),
    );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../config/examples/astm.toml")
    );
    let snapshot = analyte_config::Draft::load(
        ("machine", &definition),
        ("site", include_str!("../../../config/examples/site.toml")),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    Ok(Machine {
        #[cfg(feature = "test-hooks")]
        law_committed: None,
        #[cfg(feature = "test-hooks")]
        astm_committed: None,
        id: analyte_model::Identifier::new("synthetic")?,
        snapshot,
        listener: None,
    })
}

#[tokio::test]
async fn lis02_section11_adr0004_missed_deadline_is_query_error_or_silence_never_no_information()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let limits = analyte_store::Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: Duration::from_millis(20),
    };
    let mut store = Handle::start(
        temp.path().join("orders.db"),
        limits,
        Box::new(Space),
        4,
        crate::deadline(),
    )
    .await?;
    for policy in ["error", "no_answer"] {
        let machine = machine(policy)?;
        let started = Instant::now()
            .checked_sub(Duration::from_secs(2))
            .ok_or("clock")?;
        let mut pending = Pending::new(&store, &machine, vec![SampleId::new("001a")?], started)?;
        let response = pending.finish(&machine).await?;
        if policy == "error" {
            assert!(
                response
                    .ok_or("error response absent")?
                    .records
                    .last()
                    .is_some_and(|r| r == b"L|1|Q\r")
            );
        } else {
            assert!(response.is_none());
        }
    }
    store.shutdown(crate::deadline()).await?;
    Ok(())
}

#[test]
fn lis02_section11_cancel_is_distinct_and_unsupported_filters_are_not_empty_orders()
-> Result<(), Error> {
    let machine = machine("error")?;
    assert!(matches!(
        request(
            &machine,
            b"H|\\^&|||SYN|||||||P|1\rQ|1|^001a||||||||||A\rL|1|N\r"
        )?,
        Some(Request::Cancel)
    ));
    assert!(matches!(
        request(
            &machine,
            b"H|\\^&|||SYN|||||||P|1\rQ|1|^001a|^0099|ALL||||||||O\rL|1|N\r"
        )?,
        Some(Request::Unsupported)
    ));
    Ok(())
}
