//! Tests astm behavior with synthetic fixtures.
use super::*;
use crate::Error;
use analyte_fixtures::ConfigEdit as _;
use analyte_model::Identifier;
use std::time::Duration;

use analyte_fixtures::UnlimitedSpace as Space;

fn machine(policy: &str) -> Result<Machine, Error> {
    let section = include_str!("../../../../../core/config/examples/orders.toml").rewrite(
        "when_untrusted = \"error\"",
        &format!("when_untrusted = \"{policy}\""),
    );
    let definition = format!(
        "{}\n{section}",
        include_str!("../../../../../core/config/examples/astm.toml")
    );
    let snapshot = analyte_config::Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    Ok(Machine {
        model: analyte_contract::document::ModelId::new("example", "synthetic-astm")?,
        capture: false,
        destination_limits: crate::runtime::delivery::limits(
            snapshot.effective().site().destination.as_ref(),
        ),

        hooks: crate::installation::config::MachineHooks::default(),
        id: Identifier::new("synthetic")?,
        status: crate::machine::link::status::Reporter::new(
            crate::machine::link::status::Link::of(snapshot.effective().stack()),
        ),
        snapshot,
        listener: None,
        definition: std::path::PathBuf::new(),
    })
}

fn queries(machine: &Machine) -> Result<Queries<'_>, Error> {
    let analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(stack)) =
        machine.snapshot.effective().stack()
    else {
        return Err("ASTM fixture".into());
    };
    Ok(Queries::new(stack).ok_or("ASTM query fixture")?)
}

// CLSI LIS02-A2 §11; ADR 0004
#[tokio::test]
async fn missed_deadline_never_answers_no_information() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let limits = {
        let mut limits = analyte_fixtures::store_limits();
        limits.busy_timeout = Duration::from_millis(20);
        limits
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
        let mut pending = Pending::new(
            &store,
            &machine,
            queries(&machine)?,
            vec![SampleId::new("001a")?],
            started,
        )?;
        assert_eq!(
            pending.deadline(),
            started + Duration::from_millis(u64::from(queries(&machine)?.policy.deadline_ms)),
            "ADR 0004: admission and journaling consume the real query budget"
        );
        let response = pending.finish().await?;
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

// CLSI LIS02-A2 §11
#[test]
fn cancel_and_unsupported_filters_are_not_empty_orders() -> Result<(), Error> {
    let machine = machine("error")?;
    // LIS02-A2 section 6.13 p.13: the edition is LIS2-A2, not a legacy numeric version.
    assert!(
        envelope(
            queries(&machine)?.orders,
            Vec::new(),
            analyte_astm::OrderTermination::QueryError,
            false,
        )[0]
        .starts_with(b"H|\\^&|||Analyte|||||||P|LIS2-A2|")
    );
    assert!(matches!(
        request(
            queries(&machine)?,
            b"H|\\^&|||SYN|||||||P|1\rQ|1|^001a||||||||||A\rL|1|N\r"
        )?,
        Some(Request::Cancel)
    ));
    assert!(matches!(
        request(
            queries(&machine)?,
            b"H|\\^&|||SYN|||||||P|1\rQ|1|^001a|^0099|ALL||||||||O\rL|1|N\r"
        )?,
        Some(Request::Unsupported)
    ));
    Ok(())
}

// ADR 0004 / safety rule 9 / LIS02 section 11: unavailable is query error, never no information.
// Safety rule 9 (ADR 0003)
#[tokio::test]
async fn untrusted_order_cache_answers_query_error() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    for case in ["unknown", "incomplete", "stale", "paused", "short", "long"] {
        let mut machine = machine("error")?;
        machine.destination_limits.freshness =
            Duration::from_secs(if case == "long" { 120 } else { 10 });
        let mut store = Handle::start(
            temp.path().join(format!("{case}.db")),
            {
                let mut limits = analyte_fixtures::store_limits();
                limits.busy_timeout = Duration::from_millis(20);
                limits
            },
            Box::new(Space),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        let destination = crate::orders::destination(&machine)?;
        store
            .request::<_, analyte_store::OrderError>(crate::deadline()?, move |s| {
                if case != "unknown" {
                    s.order_page(&analyte_store::OrderPage {
                        destination,
                        cursor: Identifier::new("c").map_err(|cause| {
                            analyte_store::OrderError::Storage(analyte_store::StorageError::Text(
                                cause,
                            ))
                        })?,
                        complete: case != "incomplete",
                        polled: jiff::Timestamp::now().as_second()
                            - if case == "paused" { 0 } else { 50 },
                        orders: Vec::new(),
                    })?;
                }
                if case == "paused" {
                    s.audit(&analyte_store::Audit {
                        actor: Identifier::new("operator").map_err(|cause| {
                            analyte_store::OrderError::Storage(analyte_store::StorageError::Text(
                                cause,
                            ))
                        })?,
                        action: Identifier::new("store-restore").map_err(|cause| {
                            analyte_store::OrderError::Storage(analyte_store::StorageError::Text(
                                cause,
                            ))
                        })?,
                        targets: Vec::new(),
                        time: 0,
                        reason: analyte_model::Reason::new("synthetic restore").map_err(
                            |cause| {
                                analyte_store::OrderError::Storage(
                                    analyte_store::StorageError::Text(cause),
                                )
                            },
                        )?,
                    })?;
                }
                Ok::<_, analyte_store::OrderError>(())
            })
            .await?;
        let mut pending = Pending::new(
            &store,
            &machine,
            queries(&machine)?,
            vec![SampleId::new("001a")?],
            Instant::now(),
        )?;
        let response = pending.finish().await?.ok_or("query response absent")?;
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
// CLSI LIS02-A2 §6.14, p. 13
#[test]
fn generated_header_has_utc_time() -> Result<(), Error> {
    let machine = machine("error")?;
    let before = jiff::Timestamp::now().strftime("%Y%m%d%H%M%S").to_string();
    let records = envelope(
        queries(&machine)?.orders,
        Vec::new(),
        analyte_astm::OrderTermination::Normal,
        false,
    );
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

// CLSI LIS02-A2 §6.10
#[test]
fn generated_header_identifies_receiver() -> Result<(), Error> {
    for receiver in [None, Some("Peer|A")] {
        let mut machine = machine("error")?;
        if let Some(receiver) = receiver {
            let section = include_str!("../../../../../core/config/examples/orders.toml").rewrite(
                "[orders.astm]",
                &format!("[orders.astm]\nreceiver={receiver:?}"),
            );
            let definition = format!(
                "{}\n{section}",
                include_str!("../../../../../core/config/examples/astm.toml")
            );
            machine.snapshot = analyte_config::Draft::load(
                ("machine", &definition),
                (
                    "site",
                    include_str!("../../../../../core/config/examples/site.toml"),
                ),
                ("overrides", "schema_version=1"),
            )?
            .validate()?;
        }
        let records = envelope(
            queries(&machine)?.orders,
            Vec::new(),
            analyte_astm::OrderTermination::Normal,
            false,
        );
        let messages = analyte_astm::parse(&records.concat(), analyte_astm::Limits::default())?;
        let header = analyte_astm::Header::new(&messages[0].records()[0]).ok_or("header")?;
        assert_eq!(
            header.receiver().ok_or("receiver")?.repeats()[0][0].text(),
            receiver.unwrap_or("")
        );
    }
    Ok(())
}

// CLSI LIS02-A2 §5.4.5, §8.4.3
#[test]
fn query_decodes_specimen_and_refuses_unknown_operations() -> Result<(), Error> {
    let machine = machine("error")?;
    for (raw, expected) in [
        ("A&F&B", "A|B"),
        ("A&S&B", "A^B"),
        ("A&R&B", "A\\B"),
        ("A&E&B", "A&B"),
    ] {
        let payload = format!("H|\\^&||||||||||P|LIS2-A2\rQ|1|^{raw}||||||||||O\rL|1|N\r");
        let Some(Request::Orders(samples)) = request(queries(&machine)?, payload.as_bytes())?
        else {
            return Err("query rejected".into());
        };
        assert_eq!(samples[0].as_str(), expected);
    }
    assert!(
        request(
            queries(&machine)?,
            b"H|\\^&||||||||||P|LIS2-A2\rQ|1|^A&Zlocal&B||||||||||O\rL|1|N\r"
        )
        .is_err()
    );
    Ok(())
}

// ADR 0003 rule 8
#[tokio::test]
async fn late_send_ahead_reservation_is_settled_on_disconnect() -> Result<(), Error> {
    late_reservation_is_settled(false).await
}

// ADR 0003 rule 8; CLSI LIS02-A2 §11
#[tokio::test]
async fn late_query_reservation_is_settled_on_disconnect() -> Result<(), Error> {
    late_reservation_is_settled(true).await
}

type Gate = (
    tokio::sync::oneshot::Sender<()>,
    std::sync::mpsc::Receiver<()>,
);

/// Pauses reservation inside the worker's capacity check, before the commit.
#[derive(Clone, Default)]
struct ReservationProbe(std::sync::Arc<std::sync::Mutex<Option<Gate>>>);

impl analyte_store::FreeSpace for ReservationProbe {
    fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
        let gate = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("probe poisoned"))?
            .take();
        if let Some((entered, release)) = gate {
            entered
                .send(())
                .map_err(|()| std::io::Error::other("gate observer gone"))?;
            release
                .recv_timeout(Duration::from_secs(30))
                .map_err(std::io::Error::other)?;
        }
        Ok(u64::MAX)
    }
}

async fn late_reservation_is_settled(query: bool) -> Result<(), Error> {
    use super::super::{Context, Effects, Event, Host, Query};
    use analyte_model::OrderState;

    let temp = tempfile::tempdir()?;
    let machine = machine("error")?;
    let connection = crate::intake::ConnectionEvidence::new(&machine)?;
    let analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(stack)) =
        machine.snapshot.effective().stack()
    else {
        return Err("ASTM fixture".into());
    };
    let probe = ReservationProbe::default();
    let mut store = Handle::start(
        temp.path().join("orders.db"),
        analyte_fixtures::store_limits(),
        Box::new(probe.clone()),
        4,
        crate::test_startup_deadline()?,
    )
    .await?;
    let candidate = seed_reservation_order(&store, &machine).await?;
    let key = candidate.identity.clone();

    let mut host = Host::open(&connection, stack, &store)?;
    let (entered, wait_for_worker) = tokio::sync::oneshot::channel();
    let (release_worker, release) = std::sync::mpsc::channel();
    *probe.0.lock().map_err(|_| "probe poisoned")? = Some((entered, release));
    let order_record = b"O|1|000Ab12\r".to_vec();
    if query {
        host.query = Query::Answer {
            reply: Some(Reply {
                candidates: vec![candidate],
                records: vec![order_record.clone()],
            }),
            until: crate::deadline()?,
        };
    }
    let (output, entered) = tokio::join!(
        host.outgoing_events(Vec::new()),
        tokio::time::timeout(Duration::from_secs(30), wait_for_worker),
    );
    entered??;
    let mut result = Err(output.expect_err("late reservation must not produce wire records"));
    assert!(matches!(
        &result,
        Err(super::Error::Orders(crate::orders::OrderError::Store(
            analyte_store::OrderError::Worker(analyte_store::WorkerError::ResponseTimeout(_))
        )))
    ));
    release_worker.send(())?;
    let read_state = |key| async {
        store
            .request::<_, analyte_store::OrderError>(
                crate::deadline().map_err(crate::orders::OrderError::from)?,
                move |store| Ok(store.machine_order(&key)?.map(|record| record.state)),
            )
            .await
            .map_err(Error::from)
    };
    assert_eq!(read_state(key.clone()).await?, Some(OrderState::Sending));
    let (mut wire, _peer) =
        analyte_transport::connection(analyte_transport::Limits::new(4, Duration::from_secs(1))?);
    let (_, stop) = tokio::sync::watch::channel(false);
    host.close(
        &mut Context {
            conn: &mut wire,
            stop,
        },
        &mut result,
    )
    .await?;
    assert_eq!(read_state(key.clone()).await?, Some(OrderState::Uncertain));
    drop(host);

    let mut next_link = Host::open(&connection, stack, &store)?;
    if query {
        next_link.query = Query::Answer {
            reply: Some(Reply {
                candidates: vec![Candidate {
                    identity: key,
                    version: std::num::NonZeroU64::MIN,
                }],
                records: vec![order_record.clone()],
            }),
            until: crate::deadline()?,
        };
    }
    let events = next_link.outgoing_events(Vec::new()).await?;
    assert!(events.iter().all(
        |event| !matches!(event, Event::Send(records) if !query || records.contains(&order_record))
    ));
    drop(next_link);
    store.shutdown(crate::deadline()?).await?;
    Ok(())
}

async fn seed_reservation_order(store: &Handle, machine: &Machine) -> Result<Candidate, Error> {
    let order = Identifier::new("order-1")?;
    let candidate =
        crate::orders::reservation::pending(machine, order.clone(), std::num::NonZeroU64::MIN)?;
    let destination = candidate.identity.destination.clone();
    store
        .request::<_, analyte_store::OrderError>(crate::deadline()?, move |store| {
            let now = jiff::Timestamp::now().as_second();
            store.order_page(&analyte_store::OrderPage {
                destination,
                cursor: Identifier::new("cursor").map_err(analyte_store::StorageError::Text)?,
                complete: true,
                polled: now,
                orders: vec![analyte_store::CachedOrder {
                    id: order,
                    sample: SampleId::new("000Ab12").map_err(analyte_store::StorageError::Text)?,
                    document: include_str!("../../../../../../docs/contract/vectors/order.json")
                        .to_owned(),
                    tombstone: false,
                    expiry: now.saturating_add(120),
                }],
            })?;
            Ok(())
        })
        .await?;
    Ok(candidate)
}
