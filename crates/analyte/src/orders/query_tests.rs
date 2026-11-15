use super::*;
use analyte_model::Identifier;
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
        capture: false,
        destination_limits: crate::delivery::limits(30),
        #[cfg(feature = "test-hooks")]
        law_committed: None,
        #[cfg(feature = "test-hooks")]
        astm_committed: None,
        id: Identifier::new("synthetic")?,
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
    // LIS02-A2 ?6.13 p.13: the edition is LIS2-A2, not a legacy numeric version.
    assert_eq!(
        envelope(Vec::new(), b'Q')[0],
        b"H|\\^&|||Analyte|||||||P|LIS2-A2\r"
    );
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

// ADR 0004 / safety rule 9 / LIS02 section 11: unavailable is query error, never no information.
#[tokio::test]
async fn rule9_astm_all_untrusted_states_and_destination_freshness() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    for case in ["unknown", "incomplete", "stale", "paused", "short", "long"] {
        let mut machine = machine("error")?;
        machine.destination_limits.freshness =
            Duration::from_secs(if case == "long" { 120 } else { 10 });
        let mut store = Handle::start(
            temp.path().join(format!("{case}.db")),
            analyte_store::Limits {
                reserve_bytes: 1024,
                database_bytes: 100_000_000,
                record_bytes: 65_536,
                page_rows: 100,
                busy_timeout: Duration::from_millis(20),
            },
            Box::new(Space),
            4,
            crate::deadline(),
        )
        .await?;
        let destination = crate::orders::destination(&machine);
        store
            .request(crate::deadline(), move |s| {
                if case != "unknown" {
                    s.order_page(&analyte_store::OrderPage {
                        destination,
                        cursor: Identifier::new("c")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        complete: case != "incomplete",
                        polled: jiff::Timestamp::now().as_second()
                            - if case == "paused" { 0 } else { 50 },
                        orders: Vec::new(),
                    })?;
                }
                if case == "paused" {
                    s.audit(&analyte_store::Audit {
                        actor: Identifier::new("operator")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        action: Identifier::new("store-restore")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                        targets: Vec::new(),
                        time: 0,
                        reason: analyte_model::Reason::new("synthetic restore")
                            .map_err(|_| analyte_store::Error::InvalidInput)?,
                    })?;
                }
                Ok(())
            })
            .await?;
        let mut pending = Pending::new(
            &store,
            &machine,
            vec![SampleId::new("001a")?],
            Instant::now(),
        )?;
        let response = pending
            .finish(&machine)
            .await?
            .ok_or("query response absent")?;
        assert_eq!(
            response.records.last().ok_or("termination absent")?,
            if case == "long" {
                b"L|1|I\r".as_slice()
            } else {
                b"L|1|Q\r".as_slice()
            },
            "{case}"
        );
        drop(pending);
        store.shutdown(crate::deadline()).await?;
    }
    Ok(())
}
