//! Tests astm behavior with synthetic fixtures.
use super::*;
use crate::Error;
use analyte_model::Identifier;
use std::time::Duration;

use analyte_fixtures::UnlimitedSpace as Space;

fn machine(policy: &str) -> Result<Machine, Error> {
    let section = include_str!("../../../../core/config/examples/orders.toml").replace(
        "when_untrusted = \"error\"",
        &format!("when_untrusted = \"{policy}\""),
    );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../../core/config/examples/astm.toml")
    );
    let snapshot = analyte_config::Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    Ok(Machine {
        capture: false,
        destination_limits: crate::runtime::delivery::limits(30),
        #[cfg(feature = "test-hooks")]
        hooks: crate::installation::config::MachineHooks::default(),
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
        crate::test_startup_deadline()?,
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
    store.shutdown(crate::deadline()?).await?;
    Ok(())
}

#[test]
fn lis02_section11_cancel_is_distinct_and_unsupported_filters_are_not_empty_orders()
-> Result<(), Error> {
    let machine = machine("error")?;
    // LIS02-A2 section 6.13 p.13: the edition is LIS2-A2, not a legacy numeric version.
    assert!(
        envelope(
            &machine,
            Vec::new(),
            analyte_astm::OrderTermination::QueryError
        )[0]
        .starts_with(b"H|\\^&|||Analyte|||||||P|LIS2-A2|")
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
            crate::test_startup_deadline()?,
        )
        .await?;
        let destination = crate::orders::destination(&machine);
        store
            .request(crate::deadline()?, move |s| {
                if case != "unknown" {
                    s.order_page(&analyte_store::OrderPage {
                        destination,
                        cursor: Identifier::new("c").map_err(analyte_store::Error::Text)?,
                        complete: case != "incomplete",
                        polled: jiff::Timestamp::now().as_second()
                            - if case == "paused" { 0 } else { 50 },
                        orders: Vec::new(),
                    })?;
                }
                if case == "paused" {
                    s.audit(&analyte_store::Audit {
                        actor: Identifier::new("operator").map_err(analyte_store::Error::Text)?,
                        action: Identifier::new("store-restore")
                            .map_err(analyte_store::Error::Text)?,
                        targets: Vec::new(),
                        time: 0,
                        reason: analyte_model::Reason::new("synthetic restore")
                            .map_err(analyte_store::Error::Text)?,
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
        store.shutdown(crate::deadline()?).await?;
    }
    Ok(())
}

// LIS02-A2 section 6.14 p.13 and section 5.6.2 p.10: generated UTC date/time to seconds.
#[test]
fn lis02_6_14_p13_generated_header_time() -> Result<(), Error> {
    let machine = machine("error")?;
    let before = jiff::Timestamp::now().strftime("%Y%m%d%H%M%S").to_string();
    let records = envelope(&machine, Vec::new(), analyte_astm::OrderTermination::Normal);
    let header = std::str::from_utf8(&records[0])?;
    let timestamp = header
        .trim_end_matches('\r')
        .split('|')
        .nth(13)
        .ok_or("missing H-14")?;
    let after = jiff::Timestamp::now().strftime("%Y%m%d%H%M%S").to_string();
    assert_eq!(timestamp.len(), 19);
    assert!(timestamp.ends_with("+0000"));
    let digits = timestamp.get(..14).ok_or("short time")?;
    assert!(digits.bytes().all(|b| b.is_ascii_digit()));
    assert!(digits >= before.as_str() && digits <= after.as_str());
    Ok(())
}

#[test]
fn lis02_6_10_generated_header_identifies_receiver() -> Result<(), Error> {
    for receiver in [None, Some("Peer|A")] {
        let mut machine = machine("error")?;
        if let Some(receiver) = receiver {
            let section = include_str!("../../../../core/config/examples/orders.toml").replace(
                "[orders.astm]",
                &format!("[orders.astm]\nreceiver={receiver:?}"),
            );
            let definition = format!(
                "{}\n{section}",
                include_str!("../../../../core/config/examples/astm.toml")
            );
            machine.snapshot = analyte_config::Draft::load(
                ("machine", &definition),
                (
                    "site",
                    include_str!("../../../../core/config/examples/site.toml"),
                ),
                ("overrides", "schema_version=1"),
            )?
            .validate()?;
        }
        let records = envelope(&machine, Vec::new(), analyte_astm::OrderTermination::Normal);
        let messages = analyte_astm::parse(&records.concat(), analyte_astm::Limits::default())?;
        let header = analyte_astm::Header::new(&messages[0].records()[0]).ok_or("header")?;
        assert_eq!(
            header.receiver().ok_or("receiver")?.repeats()[0][0].text(),
            receiver.unwrap_or("")
        );
    }
    Ok(())
}

#[test]
fn lis02_5_4_5_8_4_3_query_uses_decoded_specimen_and_refuses_unknown_operations()
-> Result<(), Error> {
    let machine = machine("error")?;
    for (raw, expected) in [
        ("A&F&B", "A|B"),
        ("A&S&B", "A^B"),
        ("A&R&B", "A\\B"),
        ("A&E&B", "A&B"),
    ] {
        let payload = format!("H|\\^&||||||||||P|LIS2-A2\rQ|1|^{raw}||||||||||O\rL|1|N\r");
        let Some(Request::Orders(samples)) = request(&machine, payload.as_bytes())? else {
            return Err("query rejected".into());
        };
        assert_eq!(samples[0].as_str(), expected);
    }
    assert!(
        request(
            &machine,
            b"H|\\^&||||||||||P|LIS2-A2\rQ|1|^A&Zlocal&B||||||||||O\rL|1|N\r"
        )
        .is_err()
    );
    Ok(())
}
