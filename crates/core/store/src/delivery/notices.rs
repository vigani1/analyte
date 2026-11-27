//! Finds the Needs review items a lab app should be told about, queues each notice once in the outbox and stops when the lab app declines them (ADR 0037).
//!
//! A notice tells a lab app that an item waits for review in Analyte, and later
//! that it was settled. It never carries a raw message or a value: only what a
//! lab app needs to find the sample, which it already owns. The decision stays
//! in Analyte.
use crate::{
    Audit, DeliveryError as Error, DeliveryKind, DeliveryState, Interpretation, OrderingKey,
    Parked, Reading, Store, digest, key,
};
use analyte_model::{
    DeliveryId, Identifier, IdentityStatus, NamespacedId, Reason, ReceiptMetadata, ResultContent,
    SampleId,
};
use rusqlite::{Connection, OptionalExtension, params};

/// Most test codes one notice names: a contract evidence list.
pub const REVIEW_NOTICE_TESTS: usize = 256;

/// Whether a notice opens an item for a lab app or settles it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewPhase {
    /// The item waits for review in Analyte.
    Opened,
    /// Staff settled the item in Analyte.
    Closed,
}

impl ReviewPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Closed => "closed",
        }
    }
}

/// One Needs review item as a notice describes it, read from what the store
/// keeps beside it. Nothing here is a raw message or a value.
pub struct ReviewSubject {
    /// The parked item's number, the stable reference a notice names.
    pub item: i64,
    /// Its kind, reason and linked records, as stored.
    pub details: Parked,
    /// When it was parked; none when an older version did not record it.
    pub parked_at: Option<i64>,
    /// The machine the item came from.
    pub machine: Identifier,
    /// The connection its message arrived on, when a receipt names it.
    pub connection: Option<Identifier>,
    /// The sample, exactly as received, when the item concerns exactly one.
    pub sample: Option<SampleId>,
    /// The machine's own test codes the item concerns, in source order, each
    /// once, at most [`REVIEW_NOTICE_TESTS`].
    pub tests: Vec<NamespacedId>,
    /// How it was settled; present only for a closing notice.
    pub resolution: Option<Audit>,
}

/// What one lab app should be told now: items to open, items to close, and
/// how far the item list was read. Pass `scanned` back with the notices.
#[derive(Default)]
pub struct ReviewWork {
    /// Open items parked after the last check, for this lab app.
    pub opened: Vec<ReviewSubject>,
    /// Items this lab app was told about that staff have since settled.
    pub closed: Vec<ReviewSubject>,
    /// The last item number read for opening notices.
    pub scanned: i64,
}

/// One notice's frozen bytes, for the outbox. Its ID is the notice ID and
/// stays the same on every retry.
pub struct ReviewNoticeDelivery {
    /// Stable delivery identity, also the notice's own ID.
    pub id: DeliveryId,
    /// The lab app it goes to.
    pub destination: Identifier,
    /// The parked item it is about.
    pub item: i64,
    /// Opening or closing.
    pub phase: ReviewPhase,
    /// Exact contract bytes.
    pub payload: Vec<u8>,
    /// Captured wire contract version.
    pub contract_version: u32,
}

/// Whether a lab app that takes notices declined them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewNoticeStatus {
    /// When and why the lab app declined notices (it answered 404 or 501);
    /// none while it takes them.
    pub declined: Option<(i64, Reason)>,
}

impl Store {
    /// Sets which lab apps take notices, at engine start. A lab app newly on is
    /// told about items parked from now on; one turned off forgets where it
    /// was. A lab app that declined notices is asked again, once, because its
    /// setting is still on (ADR 0037). Notices already queued are kept.
    /// # Errors
    /// Refuses more than 128 lab apps and reports database failures.
    pub fn configure_review_notices(&mut self, destinations: &[Identifier]) -> Result<(), Error> {
        if destinations.len() > 128 {
            return Err(Error::InvalidDeliveryInput);
        }
        let names = serde_json::to_string(
            &destinations
                .iter()
                .map(Identifier::as_str)
                .collect::<Vec<_>>(),
        )?;
        let tx = self.db.transaction()?;
        tx.execute(
            "DELETE FROM review_notice_destinations WHERE destination NOT IN (SELECT value FROM json_each(?1))",
            [&names],
        )?;
        tx.execute(
            "UPDATE review_notice_destinations SET declined_at=NULL,declined_reason=NULL",
            [],
        )?;
        tx.execute(
            "INSERT INTO review_notice_destinations(destination,scanned) SELECT value,(SELECT coalesce(max(id),0) FROM parked) FROM json_each(?1) WHERE true ON CONFLICT(destination) DO NOTHING",
            [&names],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Whether `destination` takes notices and whether it declined them; none
    /// when its setting is off.
    /// # Errors
    /// Reports database failures and corrupt stored reasons.
    pub fn review_notice_status(
        &self,
        destination: &Identifier,
    ) -> Result<Option<ReviewNoticeStatus>, Error> {
        let row: Option<(Option<i64>, Option<String>)> = self
            .db
            .query_row(
                "SELECT declined_at,declined_reason FROM review_notice_destinations WHERE destination=?1",
                [destination.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(at, reason)| {
            Ok(ReviewNoticeStatus {
                declined: match (at, reason) {
                    (Some(at), Some(reason)) => {
                        Some((at, Reason::new(&reason).map_err(|_| Error::Corruption)?))
                    }
                    (None, None) => None,
                    _ => return Err(Error::Corruption),
                },
            })
        })
        .transpose()
    }

    /// What `destination` should be told now, at most one page of each kind.
    /// `machines` are the machines that send to it: an item that names no lab
    /// app of its own (a message, a machine's question) goes to its machine's
    /// lab app. Items about a notice itself, items Analyte cannot trace to a
    /// machine, and items already settled before they were read are skipped.
    /// None when the lab app does not take notices or declined them.
    /// # Errors
    /// Refuses more than 128 machines; reports database failures and corrupt evidence.
    pub fn review_notice_work(
        &self,
        destination: &Identifier,
        machines: &[Identifier],
    ) -> Result<Option<ReviewWork>, Error> {
        if machines.len() > 128 {
            return Err(Error::InvalidDeliveryInput);
        }
        let progress: Option<(i64, Option<i64>)> = self
            .db
            .query_row(
                "SELECT scanned,declined_at FROM review_notice_destinations WHERE destination=?1",
                [destination.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((scanned, None)) = progress else {
            return Ok(None);
        };
        let rows = i64::try_from(self.limits.page_rows).map_err(|_| Error::InvalidDeliveryInput)?;
        let mut work = ReviewWork {
            scanned,
            ..ReviewWork::default()
        };
        let items: Vec<(i64, String, Option<i64>, bool)> = self
            .db
            .prepare("SELECT id,evidence,parked_at,resolution IS NOT NULL FROM parked WHERE id>?1 ORDER BY id LIMIT ?2")?
            .query_map(params![scanned, rows], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<Result<_, _>>()?;
        for (item, evidence, parked_at, resolved) in items {
            work.scanned = item;
            if resolved {
                continue;
            }
            if let Some(subject) =
                self.subject(item, &evidence, parked_at, None, destination, machines)?
            {
                work.opened.push(subject);
            }
        }
        let settled: Vec<(i64, String, Option<i64>, String)> = self
            .db
            .prepare(concat!(
                "SELECT p.id,p.evidence,p.parked_at,p.resolution FROM review_notices_open w ",
                "JOIN outbox o ON o.id=w.opening JOIN parked p ON p.id=w.item ",
                "WHERE w.destination=?1 AND o.state=?2 AND p.resolution IS NOT NULL ORDER BY p.id LIMIT ?3",
            ))?
            .query_map(
                params![destination.as_str(), DeliveryState::Done, rows],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?
            .collect::<Result<_, _>>()?;
        for (item, evidence, parked_at, resolution) in settled {
            // A resolution another version wrote in a shape this one cannot
            // read is not guessed at: the item stays open for the lab app.
            let Ok(resolution) = serde_json::from_str::<Audit>(&resolution) else {
                continue;
            };
            // The opening notice reached this lab app, so the closing one goes
            // there too, even if the machine has since moved.
            if let Some(subject) = self.subject(
                item,
                &evidence,
                parked_at,
                Some(resolution),
                destination,
                &[],
            )? {
                work.closed.push(subject);
            }
        }
        Ok(Some(work))
    }

    /// Queues `notices` for `destination` and records `scanned`, in one write.
    /// An item already given a notice of the same phase gets none again, so
    /// each item has at most one opening and one closing notice per lab app.
    /// Returns how many were queued.
    /// # Errors
    /// Refuses a notice for another lab app, a closing notice without a
    /// delivered opening one, oversized bytes, insufficient capacity and
    /// database failures; nothing is written then.
    pub fn enqueue_review_notices(
        &mut self,
        destination: &Identifier,
        notices: &[ReviewNoticeDelivery],
        scanned: i64,
    ) -> Result<usize, Error> {
        let mut required = 256usize;
        for notice in notices {
            if notice.destination != *destination {
                return Err(Error::InvalidDeliveryInput);
            }
            if notice.payload.len() > self.limits.record_bytes {
                return Err(crate::StorageError::LimitExceeded {
                    what: "record_bytes",
                }
                .into());
            }
            required = required
                .checked_add(notice.payload.len())
                .and_then(|bytes| bytes.checked_add(512))
                .ok_or(crate::StorageError::Capacity)?;
        }
        Self::capacity_on(&self.db, &self.path, &self.limits, &*self.space, required)?;
        let tx = self.db.transaction()?;
        let mut queued = 0;
        for notice in notices {
            let given: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM review_notices WHERE destination=?1 AND item=?2 AND phase=?3)",
                params![destination.as_str(), notice.item, notice.phase.as_str()],
                |r| r.get(0),
            )?;
            if given {
                continue;
            }
            let id = key(&notice.id)?;
            let opening: Option<String> = tx
                .query_row(
                    "SELECT opening FROM review_notices_open WHERE destination=?1 AND item=?2",
                    params![destination.as_str(), notice.item],
                    |r| r.get(0),
                )
                .optional()?;
            if notice.phase == ReviewPhase::Closed && opening.is_none() {
                return Err(Error::InvalidDeliveryInput);
            }
            tx.execute(
                "INSERT INTO outbox(id,kind,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,unixepoch())",
                params![
                    id,
                    DeliveryKind::ReviewNotice,
                    destination.as_str(),
                    notice.item.to_string(),
                    notice.payload,
                    digest(&notice.payload).0.as_slice(),
                    notice.contract_version,
                    DeliveryState::Pending
                ],
            )?;
            tx.execute(
                "INSERT INTO review_notices(delivery,destination,item,phase) VALUES (?1,?2,?3,?4)",
                params![id, destination.as_str(), notice.item, notice.phase.as_str()],
            )?;
            match notice.phase {
                ReviewPhase::Opened => {
                    tx.execute(
                        "INSERT INTO review_notices_open(destination,item,opening) VALUES (?1,?2,?3)",
                        params![destination.as_str(), notice.item, id],
                    )?;
                }
                ReviewPhase::Closed => {
                    tx.execute(
                        "DELETE FROM review_notices_open WHERE destination=?1 AND item=?2",
                        params![destination.as_str(), notice.item],
                    )?;
                }
            }
            queued += 1;
        }
        tx.execute(
            "UPDATE review_notice_destinations SET scanned=max(scanned,?2) WHERE destination=?1",
            params![destination.as_str(), scanned],
        )?;
        tx.commit()?;
        Ok(queued)
    }

    /// Reads what a notice says about one item, or none when the item does
    /// not belong to `destination` or cannot be traced to a machine.
    fn subject(
        &self,
        item: i64,
        evidence: &str,
        parked_at: Option<i64>,
        resolution: Option<Audit>,
        destination: &Identifier,
        machines: &[Identifier],
    ) -> Result<Option<ReviewSubject>, Error> {
        let evidence: crate::parked::ParkingEvidence = serde_json::from_str(evidence)?;
        let Some(origin) = origin(&self.db, item, &evidence)? else {
            return Ok(None);
        };
        // A closing notice follows its opening one; an opening one goes to the
        // lab app the item names, else to its machine's lab app.
        let belongs = resolution.is_some()
            || match &origin.destination {
                Some(named) => named == destination,
                None => machines.contains(&origin.machine),
            };
        if !belongs {
            return Ok(None);
        }
        let (sample, tests) = summary(&origin.results);
        Ok(Some(ReviewSubject {
            item,
            details: evidence.details,
            parked_at,
            machine: origin.machine,
            connection: origin.connection,
            sample: sample.or(origin.sample),
            tests,
            resolution,
        }))
    }
}

/// Where an item came from, as far as the store can tell.
struct Origin {
    machine: Identifier,
    connection: Option<Identifier>,
    /// The lab app the item's own record names: a delivery or a machine order.
    destination: Option<Identifier>,
    /// The results it concerns.
    results: Vec<ResultContent>,
    /// The sample its order names, when it concerns an order.
    sample: Option<SampleId>,
}

/// Traces an item to its machine through its linked records: a delivery, a
/// receipt, a machine order, a transfer or a machine's question. The first
/// target that traces wins; an item about a notice traces to nothing.
fn origin(
    db: &Connection,
    item: i64,
    evidence: &crate::parked::ParkingEvidence,
) -> Result<Option<Origin>, Error> {
    for target in &evidence.details.targets {
        let target_key = serde_json::to_string(target.as_str())?;
        let delivery: Option<(DeliveryKind, Option<String>, Option<i64>, Option<String>, String, String)> = db
            .query_row(
                "SELECT kind,receipt,version,order_id,destination,ordering_key FROM outbox WHERE id=?1",
                [&target_key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;
        if let Some((kind, receipt, version, order, named, ordering)) = delivery {
            let named = Identifier::new(&named).map_err(|_| Error::Corruption)?;
            return match kind {
                DeliveryKind::ReviewNotice => Ok(None),
                DeliveryKind::Result => {
                    let (Some(receipt), Some(version)) = (receipt, version) else {
                        return Err(Error::Corruption);
                    };
                    let Some(metadata) = metadata(db, &receipt)? else {
                        return Ok(None);
                    };
                    let ordering: OrderingKey = serde_json::from_str(&ordering)?;
                    let results = results(db, &receipt, Some(version))?
                        .into_iter()
                        .filter(|result| {
                            matches!(&result.identity, IdentityStatus::Resolved(identity)
                                if identity.observation == ordering.observation)
                        })
                        .collect();
                    Ok(Some(Origin {
                        machine: metadata.machine,
                        connection: Some(metadata.connection),
                        destination: Some(named),
                        results,
                        sample: None,
                    }))
                }
                DeliveryKind::OrderEvent => {
                    let Some(order) = order else {
                        return Err(Error::Corruption);
                    };
                    let machines: Vec<String> = db
                        .prepare("SELECT machine FROM machine_orders WHERE destination=?1 AND order_id=?2 ORDER BY machine LIMIT 2")?
                        .query_map(params![named.as_str(), order], |r| r.get(0))?
                        .collect::<Result<_, _>>()?;
                    let [machine] = machines.as_slice() else {
                        return Ok(None);
                    };
                    Ok(Some(Origin {
                        machine: Identifier::new(machine).map_err(|_| Error::Corruption)?,
                        connection: None,
                        sample: order_sample(db, named.as_str(), &order)?,
                        destination: Some(named),
                        results: Vec::new(),
                    }))
                }
            };
        }
        if let Some(metadata) = metadata(db, &target_key)? {
            let results = match &evidence.context {
                Some(crate::parked::ParkingContext::Result { version, index, .. }) => {
                    results(db, &target_key, *version)?
                        .into_iter()
                        .nth(*index)
                        .into_iter()
                        .collect()
                }
                Some(crate::parked::ParkingContext::Interpretation { version }) => {
                    results(db, &target_key, Some(*version))?
                }
                None => results(db, &target_key, None)?,
            };
            return Ok(Some(Origin {
                machine: metadata.machine,
                connection: Some(metadata.connection),
                destination: None,
                results,
                sample: None,
            }));
        }
        let transfer: Option<Option<String>> = db
            .query_row(
                "SELECT receipt FROM transfers WHERE id=?1",
                [target.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(receipt) = transfer {
            let Some(metadata) = receipt.map(|receipt| metadata(db, &receipt)).transpose()?.flatten() else {
                return Ok(None);
            };
            return Ok(Some(Origin {
                machine: metadata.machine,
                connection: Some(metadata.connection),
                destination: None,
                results: Vec::new(),
                sample: None,
            }));
        }
        if let Ok(order) = target.as_str().parse::<i64>() {
            let machine_order: Option<(String, String, String)> = db
                .query_row(
                    "SELECT machine,destination,order_id FROM machine_orders WHERE id=?1",
                    [order],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some((machine, named, order)) = machine_order {
                return Ok(Some(Origin {
                    machine: Identifier::new(&machine).map_err(|_| Error::Corruption)?,
                    connection: None,
                    sample: order_sample(db, &named, &order)?,
                    destination: Some(Identifier::new(&named).map_err(|_| Error::Corruption)?),
                    results: Vec::new(),
                }));
            }
        }
    }
    let asked: Option<String> = db
        .query_row(
            "SELECT machine FROM poll_intents WHERE parked=?1 ORDER BY id LIMIT 1",
            [item],
            |r| r.get(0),
        )
        .optional()?;
    asked
        .map(|machine| {
            Ok(Origin {
                machine: Identifier::new(&machine).map_err(|_| Error::Corruption)?,
                connection: None,
                destination: None,
                results: Vec::new(),
                sample: None,
            })
        })
        .transpose()
}

fn metadata(db: &Connection, receipt_key: &str) -> Result<Option<ReceiptMetadata>, Error> {
    let stored: Option<String> = db
        .query_row(
            "SELECT metadata FROM receipts WHERE id=?1",
            [receipt_key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored.map(|text| serde_json::from_str(&text)).transpose()?)
}

/// The results one reading of a receipt holds: `version`'s, or the latest one's.
fn results(
    db: &Connection,
    receipt_key: &str,
    version: Option<i64>,
) -> Result<Vec<ResultContent>, Error> {
    let stored: Option<String> = db
        .query_row(
            "SELECT evidence FROM interpretations WHERE receipt=?1 AND (?2 IS NULL OR version=?2) ORDER BY version DESC LIMIT 1",
            params![receipt_key, version],
            |r| r.get(0),
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(Vec::new());
    };
    Ok(match serde_json::from_str::<Interpretation>(&stored)?.outcome {
        Reading::Results(results) => results,
        Reading::Failed(_) => Vec::new(),
    })
}

fn order_sample(db: &Connection, destination: &str, order: &str) -> Result<Option<SampleId>, Error> {
    let sample: Option<String> = db
        .query_row(
            "SELECT sample FROM orders WHERE destination=?1 AND id=?2",
            params![destination, order],
            |r| r.get(0),
        )
        .optional()?;
    sample
        .map(|sample| SampleId::new(&sample).map_err(|_| Error::Corruption))
        .transpose()
}

/// The one sample `results` name, if exactly one, and their test codes in
/// source order, each once, at most [`REVIEW_NOTICE_TESTS`].
fn summary(results: &[ResultContent]) -> (Option<SampleId>, Vec<NamespacedId>) {
    let mut samples: Vec<&SampleId> = Vec::new();
    let mut tests: Vec<NamespacedId> = Vec::new();
    for result in results {
        if let Some(sample) = &result.sample
            && !samples.contains(&&sample.id)
        {
            samples.push(&sample.id);
        }
        for part in result.parts.as_slice() {
            if tests.len() < REVIEW_NOTICE_TESTS && !tests.contains(&part.test) {
                tests.push(part.test.clone());
            }
        }
    }
    let sample = match samples.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    };
    (sample, tests)
}

/// Records that `delivery`'s lab app declined notices: this notice and every
/// one still waiting for it are declined, nothing more is queued for it until
/// the engine starts again, and nothing is parked. In the caller's transaction.
pub(crate) fn decline(
    tx: &rusqlite::Transaction<'_>,
    delivery: &str,
    reason: &Reason,
    now: i64,
) -> Result<(), Error> {
    let destination: String = tx.query_row(
        "SELECT destination FROM outbox WHERE id=?1",
        [delivery],
        |r| r.get(0),
    )?;
    tx.execute(
        "UPDATE outbox SET state=?3 WHERE destination=?1 AND kind=?2 AND state=?4",
        params![
            destination,
            DeliveryKind::ReviewNotice,
            DeliveryState::Declined,
            DeliveryState::Pending
        ],
    )?;
    tx.execute(
        "DELETE FROM review_notices_open WHERE destination=?1 AND opening IN (SELECT id FROM outbox WHERE destination=?1 AND state=?2)",
        params![destination, DeliveryState::Declined],
    )?;
    tx.execute(
        "UPDATE review_notice_destinations SET declined_at=?2,declined_reason=?3 WHERE destination=?1",
        params![destination, now, reason.as_str()],
    )?;
    Ok(())
}

/// Whether `delivery` is a notice: only a notice can be declined.
pub(crate) fn is_notice(tx: &rusqlite::Transaction<'_>, delivery: &str) -> Result<bool, Error> {
    Ok(tx
        .query_row(
            "SELECT kind FROM outbox WHERE id=?1",
            [delivery],
            |r| r.get::<_, DeliveryKind>(0),
        )
        .optional()?
        == Some(DeliveryKind::ReviewNotice))
}
