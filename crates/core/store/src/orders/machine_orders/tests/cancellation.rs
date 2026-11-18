//! Tests machine order cancellation transitions.
use super::*;

#[test]
fn adr0003_typed_ownership_ignores_reason_prefixes_and_cancel_replay_keeps_full_version()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("typed-owners.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let key = identity()?;
    let owner = Identifier::new("connection:part")?;
    let record = store
        .reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions {
                owner: Some(&owner),
                ..Default::default()
            },
        )?
        .map_err(std::io::Error::other)?;
    assert!(record.reserved_by(&owner));
    assert!(!record.reserved_by(&Identifier::new("part")?));
    // A peer-supplied reason that resembles the historical ownership convention
    // must not admit a host cancellation or create a replay-blocking version.
    let uncertain = store.transition_order(
        record.id,
        OrderState::Uncertain,
        SourceText::new("app-cancel:9:other")?,
        102,
        event,
    )?;
    assert!(!uncertain.cancel_requested_by(&Identifier::new("other")?));
    assert_eq!(
        store.db.query_row(
            "SELECT count(*) FROM machine_order_history WHERE cancel_version IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    store.reconcile_order(
        record.id,
        OrderState::Accepted,
        &Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("reconcile")?,
            targets: vec![Identifier::new(&record.id.to_string())?],
            time: 103,
            reason: analyte_model::Reason::new("synthetic acceptance evidence")?,
        },
        event,
    )?;
    let version = NonZeroU64::new(u64::MAX).ok_or("invalid version")?;
    store.order_page(&page(version.get(), "cancel_requested", false, 200)?)?;
    assert_eq!(
        store
            .cancel_candidates(&key.destination, &key.machine, 104, 30)?
            .len(),
        1
    );
    let cancelled = store.request_order_cancel(record.id, version, 104, &owner, 30, event)?;
    assert!(cancelled.cancel_requested_by(&owner));
    assert!(!cancelled.cancel_requested_by(&Identifier::new("part")?));
    assert!(
        store
            .cancel_candidates(&key.destination, &key.machine, 104, 30)?
            .is_empty()
    );
    assert!(matches!(
        store.request_order_cancel(record.id, version, 104, &owner, 30, event),
        Err(OrderError::Conflict)
    ));
    drop(store);
    let store = open(&path)?;
    assert!(
        store
            .machine_order(&key)?
            .ok_or("missing cancellation")?
            .cancel_requested_by(&owner)
    );
    assert!(
        store
            .cancel_candidates(&key.destination, &key.machine, 104, 30)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn adr0004_cancel_expiry_version_trust_and_builder_failure_never_reserve() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("orders.db"))?;
    // Cached versions only move forward, so each case carries a newer one.
    for (version, reserved, state, tombstone, expiry, now, expected) in [
        (
            2,
            1,
            "requested",
            false,
            200,
            101,
            SendRefusal::VersionChanged,
        ),
        (
            3,
            3,
            "cancel_requested",
            false,
            200,
            101,
            SendRefusal::NotRequested,
        ),
        (4, 4, "requested", true, 200, 101, SendRefusal::Withdrawn),
        (5, 5, "requested", false, 101, 101, SendRefusal::Expired),
        (6, 6, "requested", false, 200, 130, SendRefusal::Untrusted),
    ] {
        store.order_page(&page(version, state, tombstone, expiry)?)?;
        let version = NonZeroU64::new(reserved).ok_or("version")?;
        assert!(
            matches!(store.reserve_send(&OrderReservation { identity: &identity()?, version, now, freshness: 30 }, ReserveOptions::default())?,Err(reason) if reason == expected)
        );
    }
    store.order_page(&page(7, "requested", false, 200)?)?;
    {
        let refusal = store
            .reject_unsent(
                &OrderReservation {
                    identity: &identity()?,
                    version: NonZeroU64::new(7).ok_or("version")?,
                    now: 101,
                    freshness: 30,
                },
                SourceText::new("mapping rejected")?,
                |_| Err(OrderError::InvalidOrderInput),
            )
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, OrderError::InvalidOrderInput),
            "unexpected refusal: {refusal:?}"
        );
    }
    assert!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: None
            })?
            .is_empty()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn adr0004_missing_answers_and_cancel_all_confirm_or_refuse() -> TestResult {
    for refused in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("cancel.db"))?;
        let record = reserve_two(&mut store)?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: ItemDecision::Accepted,
            code: SourceText::new("OK").expect("constant code"),
        });
        store.finish_order_items(record.id, &outcomes, 102, event)?;
        store.transition_order(
            record.id,
            OrderState::CancelRequested,
            SourceText::new("CA")?,
            103,
            event,
        )?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: if refused && index == 1 {
                ItemDecision::CancelRefused
            } else {
                ItemDecision::CancelConfirmed
            },
            code: SourceText::new(if refused && index == 1 { "UC" } else { "CR" })
                .expect("constant code"),
        });
        let result = store.finish_order_items(record.id, &outcomes, 104, event)?;
        assert_eq!(
            result.state,
            if refused {
                OrderState::Accepted
            } else {
                OrderState::CancelConfirmed
            }
        );
        assert_eq!(
            result.source_code.as_str().contains("CancelRefused"),
            refused
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            3
        );
    }
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("missing.db"))?;
    let record = reserve_two(&mut store)?;
    let result = store.finish_order_items(
        record.id,
        &[ItemOutcome {
            index: 1,
            decision: ItemDecision::Accepted,
            code: SourceText::new("OK")?,
        }],
        102,
        event,
    )?;
    assert_eq!(result.state, OrderState::Uncertain);
    assert!(
        result
            .source_code
            .as_str()
            .contains("exchange-answer-missing")
    );
    Ok(())
}

#[test]
fn adr0004_cancel_refusal_restores_sent_and_missing_cancel_is_uncertain() -> TestResult {
    for missing in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("cancel-sent.db"))?;
        let record = reserve_two(&mut store)?;
        let outcomes = [1, 2].map(|index| ItemOutcome {
            index,
            decision: ItemDecision::Sent,
            code: SourceText::new("AA").expect("constant code"),
        });
        store.finish_order_items(record.id, &outcomes, 102, event)?;
        store.transition_order(
            record.id,
            OrderState::CancelRequested,
            SourceText::new("CA")?,
            103,
            event,
        )?;
        let mut outcomes = vec![ItemOutcome {
            index: 1,
            decision: ItemDecision::CancelRefused,
            code: SourceText::new("UC")?,
        }];
        if !missing {
            outcomes.push(ItemOutcome {
                index: 2,
                decision: ItemDecision::CancelConfirmed,
                code: SourceText::new("CR")?,
            });
        }
        let result = store.finish_order_items(record.id, &outcomes, 104, event)?;
        assert_eq!(
            result.state,
            if missing {
                OrderState::Uncertain
            } else {
                OrderState::Sent
            }
        );
        assert!(result.source_code.as_str().contains("CancelRefused"));
        assert!(
            store
                .order_candidates(&identity()?.destination, &identity()?.machine, "", 101, 30)?
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn adr0003_cancelled_query_wait_can_only_claim_its_retained_reservation_owner() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("owned.db");
    let mut store = open(&path)?;
    store.order_page(&page(1, "requested", false, 200)?)?;
    let candidate = identity()?;
    let reservation = OrderReservation {
        identity: &candidate,
        version: NonZeroU64::MIN,
        now: 101,
        freshness: 30,
    };
    let first = Identifier::new("connection-one")?;
    let second = Identifier::new("connection-two")?;
    let record = store
        .reserve_send(
            &reservation,
            ReserveOptions {
                owner: Some(&first),
                ..Default::default()
            },
        )?
        .map_err(std::io::Error::other)?;
    assert_eq!(record.source_code.as_str(), "host-reserved:connection-one");
    assert!(matches!(
        store.reserve_send(
            &reservation,
            ReserveOptions {
                owner: Some(&second),
                ..Default::default()
            }
        )?,
        Err(SendRefusal::AlreadyReserved)
    ));
    drop(store);
    let store = open(&path)?;
    let restored = store
        .machine_order(&candidate)?
        .ok_or("reservation absent")?;
    assert_eq!(
        restored.source_code.as_str(),
        "host-reserved:connection-one"
    );
    assert_eq!(restored.state, OrderState::Sending);
    assert_eq!(restored.items.len(), 1);
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn adr0003_rule8_cancel_refusal_uses_cancel_from_not_history_json() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("cancel-from.db"))?;
    let record = reserve_two(&mut store)?;
    let outcomes = [1, 2].map(|index| ItemOutcome {
        index,
        decision: ItemDecision::Sent,
        code: SourceText::new("AA").expect("constant"),
    });
    store.finish_order_items(record.id, &outcomes, 102, event)?;
    let cancelled = store.transition_order(
        record.id,
        OrderState::CancelRequested,
        SourceText::new("CA")?,
        103,
        event,
    )?;
    assert!(
        cancelled
            .items
            .iter()
            .all(|item| item.cancel_from == Some(OrderState::Sent))
    );
    // Deliberately conflicting historical aggregate: the current per-item evidence must win.
    let mut past = cancelled.clone();
    past.state = OrderState::Accepted;
    for snapshot in [&past, &cancelled] {
        store.db.execute(
            "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
            rusqlite::params![record.id, serde_json::to_string(snapshot)?],
        )?;
    }
    let outcomes = [1, 2].map(|index| ItemOutcome {
        index,
        decision: ItemDecision::CancelRefused,
        code: SourceText::new("UC").expect("constant"),
    });
    let restored = store.finish_order_items(record.id, &outcomes, 104, event)?;
    assert_eq!(restored.state, OrderState::Sent);
    assert!(
        restored
            .items
            .iter()
            .all(|item| item.state == OrderState::Sent)
    );
    Ok(())
}

// ADR 0004 / safety rule 9: all order readers, reservations and alerts share one trust decision.
#[test]
fn mutation_adr0003_rule8_order_cancel_version_and_phase_are_rechecked_independently() -> TestResult
{
    for (cached_phase, requested_version) in [("cancel_requested", 1), ("requested", 2)] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("cancel-recheck.db"))?;
        store.order_page(&page(1, "requested", false, 200)?)?;
        let key = identity()?;
        let record = store
            .reserve_send(
                &OrderReservation {
                    identity: &key,
                    version: NonZeroU64::MIN,
                    now: 101,
                    freshness: 30,
                },
                ReserveOptions::default(),
            )?
            .map_err(std::io::Error::other)?;
        store.transition_order(
            record.id,
            OrderState::Accepted,
            SourceText::new("peer-accepted")?,
            102,
            event,
        )?;
        store.order_page(&page(2, cached_phase, false, 200)?)?;
        let version = NonZeroU64::new(requested_version).ok_or("version")?;
        assert!(matches!(
            store.request_order_cancel(
                record.id,
                version,
                103,
                &Identifier::new("connection")?,
                30,
                event
            ),
            Err(OrderError::Conflict)
        ));
        let retained = store.machine_order(&key)?.ok_or("missing order")?;
        assert_eq!(retained.state, OrderState::Accepted);
        assert_eq!(
            retained.items.first().ok_or("missing item")?.state,
            OrderState::Accepted
        );
        assert_eq!(
            store.db.query_row(
                "SELECT count(*) FROM machine_order_history WHERE cancel_version IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            store
                .db
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            1
        );
    }
    Ok(())
}

#[test]
fn mutation_adr0003_rule8_order_cancel_pages_scan_past_unsent_work_and_refuse_expired_work()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("cancel-page.db"))?;
    store.limits.page_rows = 1;
    let base = identity()?;
    let mut accepted = None;
    for name in ["order-a", "order-b"] {
        let key = MachineOrderKey {
            order: Identifier::new(name)?,
            ..identity()?
        };
        let mut requested = page(1, "requested", false, 200)?;
        requested
            .orders
            .first_mut()
            .ok_or("missing cache order")?
            .id = key.order.clone();
        store.order_page(&requested)?;
        let record = store
            .reserve_send(
                &OrderReservation {
                    identity: &key,
                    version: NonZeroU64::MIN,
                    now: 101,
                    freshness: 30,
                },
                ReserveOptions::default(),
            )?
            .map_err(std::io::Error::other)?;
        if name == "order-b" {
            accepted = Some(
                store
                    .transition_order(
                        record.id,
                        OrderState::Accepted,
                        SourceText::new("accepted")?,
                        102,
                        event,
                    )?
                    .id,
            );
        }
        let mut cancelled = page(2, "cancel_requested", false, 200)?;
        cancelled
            .orders
            .first_mut()
            .ok_or("missing cache order")?
            .id = key.order;
        store.order_page(&cancelled)?;
    }
    let candidates = store.cancel_candidates(&base.destination, &base.machine, 103, 30)?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates.first().ok_or("missing cancellation")?.0.id,
        accepted.ok_or("missing accepted ID")?
    );
    for (expiry, expected) in [(103, 0), (104, 1)] {
        let mut cancelled = page(2, "cancel_requested", false, expiry)?;
        cancelled
            .orders
            .first_mut()
            .ok_or("missing cache order")?
            .id = Identifier::new("order-b")?;
        store.order_page(&cancelled)?;
        assert_eq!(
            store
                .cancel_candidates(&base.destination, &base.machine, 103, 30)?
                .len(),
            expected
        );
    }
    verify_mutation_rule8_larger_cancel_page(&mut store, &base)
}
