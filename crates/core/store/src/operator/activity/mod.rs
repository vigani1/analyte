//! Reads the Activity feed, newest first, from the tables that already record what Analyte did: receipts, parked items, delivery attempts, machine order history and the audit trail.
//!
//! Nothing here is a new record of events: each line is a row that was stored
//! anyway, with the time stored beside it. A row without a stored time (a
//! parked item from before schema 16) sorts after every dated line and keeps
//! `None`, never a guessed time. No bytes, values, test codes or typed
//! reasons leave the store; sample IDs do, as stored.
use crate::{OperatorError as Error, Parked, RecentOutcome, Store};
use analyte_model::OrderState;
use rusqlite::{OptionalExtension as _, params};
use serde::Deserialize;
#[cfg(test)]
mod tests;

/// Which kind of line a filter keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityGroup {
    /// Messages stored and results delivered.
    Results,
    /// Orders sent to machines, and how each machine took them.
    Orders,
    /// Items parked for review and deliveries that did not go through.
    Problems,
    /// What people or Analyte changed: settings, holds, resolved items.
    Changes,
}

impl ActivityGroup {
    /// Every group, in the order a page lists its filters.
    pub const ALL: [Self; 4] = [Self::Results, Self::Orders, Self::Problems, Self::Changes];

    /// The number the feed's query gives each group.
    const fn number(self) -> i64 {
        match self {
            Self::Results => 1,
            Self::Orders => 2,
            Self::Problems => 3,
            Self::Changes => 4,
        }
    }
}

/// Where a page of the feed starts: the position of the last line shown.
/// Lines sort by time, then by where they are stored, then by their row, so
/// two lines at the same second keep one order on every page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityCursor {
    /// The line's time in Unix seconds; [`i64::MIN`] for a line with none.
    pub at: i64,
    /// Which table the line comes from.
    pub source: i64,
    /// Its row in that table.
    pub key: i64,
}

impl ActivityCursor {
    /// Before every line: the first page.
    pub const NEWEST: Self = Self {
        at: i64::MAX,
        source: i64::MAX,
        key: i64::MAX,
    };
}

/// One line of the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityLine {
    /// Its position, for the next page.
    pub cursor: ActivityCursor,
    /// When it happened, in Unix seconds; none when no time was stored.
    pub at: Option<i64>,
    /// The machine it is about, as stored; none when it names no single machine.
    pub machine: Option<String>,
    /// What happened.
    pub what: Activity,
}

/// What a line says happened, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// A message from a machine was stored, with what its latest reading
    /// produced; none while it was never read.
    Stored(Option<RecentOutcome>),
    /// An item was parked for review.
    Parked {
        /// Its Needs review number.
        item: i64,
        /// Its kind, reason and links, as stored.
        details: Parked,
    },
    /// An item was resolved, by the audited action stored with it.
    Resolved {
        /// Its Needs review number.
        item: i64,
        /// The action, such as `parked-resolve`.
        action: String,
    },
    /// Analyte tried to deliver to a lab app.
    Attempt {
        /// The lab app, as stored.
        lab_app: String,
        /// How the try ended.
        outcome: AttemptOutcome,
        /// What the delivery carries.
        carries: Carries,
    },
    /// A machine order reached a new state.
    Order {
        /// The lab app's order number.
        order: String,
        /// The sample of the lab app's copy, when one is kept.
        sample: Option<String>,
        /// How many tests it holds; none when an older version did not record them.
        tests: Option<usize>,
        /// Its new state.
        state: OrderState,
    },
    /// An audited change, such as a machine's settings saved.
    Changed {
        /// Who, as recorded.
        actor: String,
        /// The action, such as `setup-save`.
        action: String,
        /// The first thing it names, such as a machine or lab app; none when none.
        target: Option<String>,
    },
}

/// How one delivery try ended, as its attempt row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// The lab app accepted it.
    Delivered,
    /// It did not go through; Analyte tries again after this Unix second.
    RetryAt(i64),
    /// The try was cut off; Analyte tries again with the same bytes.
    Interrupted,
    /// The lab app rejected it, so it waits for review.
    Rejected,
    /// The lab app's settings need repair, so sending stopped.
    Paused,
    /// The lab app holds different content under the same delivery ID.
    Conflict,
}

/// What a delivery carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carries {
    /// Results, with what their reading produced; none when it is no longer kept.
    Results(Option<RecentOutcome>),
    /// A message about an order.
    OrderMessage {
        /// The lab app's order number.
        order: String,
    },
}

/// Every line as `(at, source, key, machine, grp)`. The sources' numbers fix
/// the order of lines stored in the same second. Read-only audit entries
/// (opening original bytes, listing from the command line) are not changes,
/// and an audit entry stored as an item's resolution is already its
/// Resolved line.
macro_rules! events {
    () => {
        concat!(
            "WITH parked_machine(id, machine) AS (SELECT p.id, coalesce(",
            "(SELECT coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=json_quote(t.value)),",
            "(SELECT json_extract(r.metadata,'$.machine') FROM outbox o JOIN receipts r ON r.id=o.receipt WHERE o.id=json_quote(t.value)),",
            "(SELECT m.machine FROM machine_orders m WHERE m.id=CAST(t.value AS INTEGER) AND CAST(m.id AS TEXT)=t.value),",
            "(SELECT json_extract(f.evidence,'$.machine') FROM frame_provenance f WHERE f.transfer=t.value)",
            ") FROM json_each(p.evidence,'$.targets') t ORDER BY t.key LIMIT 1),",
            "(SELECT i.machine FROM poll_intents i WHERE i.parked=p.id)) FROM parked p),",
            "events(at, source, key, machine, grp) AS (",
            "SELECT unixepoch(json_extract(metadata,'$.received_at')),0,rowid,json_extract(metadata,'$.machine'),1 FROM receipts",
            " UNION ALL SELECT p.parked_at,1,p.id,m.machine,3 FROM parked p JOIN parked_machine m ON m.id=p.id",
            " UNION ALL SELECT json_extract(p.resolution,'$.time'),2,p.id,m.machine,4 FROM parked p JOIN parked_machine m ON m.id=p.id WHERE p.resolution IS NOT NULL",
            " UNION ALL SELECT a.time,3,a.sequence,coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=o.receipt),",
            "(SELECT CASE WHEN count(DISTINCT m.machine)=1 THEN min(m.machine) END FROM machine_orders m WHERE m.order_id=o.order_id AND m.destination=o.destination)),",
            "CASE WHEN a.outcome='\"Done\"' THEN 1 ELSE 3 END FROM attempts a JOIN outbox o ON o.id=a.delivery",
            " UNION ALL SELECT json_extract(evidence,'$.updated_at'),4,sequence,json_extract(evidence,'$.key.machine'),2 FROM (",
            "SELECT sequence,evidence,json_extract(evidence,'$.state') AS state,",
            "lag(json_extract(evidence,'$.state')) OVER (PARTITION BY machine_order ORDER BY sequence) AS was ",
            "FROM machine_order_history) WHERE was IS NULL OR was!=state",
            " UNION ALL SELECT json_extract(a.evidence,'$.time'),5,a.sequence,",
            "CASE WHEN json_extract(a.evidence,'$.action') LIKE 'setup-%' THEN json_extract(a.evidence,'$.targets[0]') END,4 ",
            "FROM audit a WHERE json_extract(a.evidence,'$.action') NOT IN ('admin-raw-view','deliveries-list','orders-list') ",
            "AND NOT EXISTS(SELECT 1 FROM parked p WHERE p.resolution=a.evidence)",
            ") "
        )
    };
}

/// One line's position, time and machine as the feed's query reads them.
type Row = (i64, i64, i64, Option<i64>, Option<String>);

impl Store {
    /// At most `limit` lines (and at most one store page) before `before`,
    /// newest first, for `machine` and `group` when given. Reading changes
    /// nothing.
    ///
    /// Every source is read whole to sort it, so the cost grows with what is
    /// kept; the admin pages bound the wait.
    /// # Errors
    /// Reports database failures, bounds or corrupt stored evidence.
    pub fn activity(
        &self,
        machine: Option<&str>,
        group: Option<ActivityGroup>,
        before: ActivityCursor,
        limit: usize,
    ) -> Result<Vec<ActivityLine>, Error> {
        let limit = i64::try_from(limit.min(self.limits.page_rows))
            .map_err(|_| Error::InvalidOperatorInput)?;
        let rows: Vec<Row> = self
            .db
            .prepare(concat!(
                events!(),
                "SELECT coalesce(at,-9223372036854775808) AS sort,source,key,at,machine FROM events ",
                "WHERE (?1 IS NULL OR machine=?1) AND (?2 IS NULL OR grp=?2) ",
                "AND (coalesce(at,-9223372036854775808),source,key)<(?3,?4,?5) ",
                "ORDER BY sort DESC,source DESC,key DESC LIMIT ?6"
            ))?
            .query_map(
                params![
                    machine,
                    group.map(ActivityGroup::number),
                    before.at,
                    before.source,
                    before.key,
                    limit
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?
            .collect::<Result<_, _>>()?;
        rows.into_iter()
            .map(|(sort, source, key, at, machine)| {
                Ok(ActivityLine {
                    cursor: ActivityCursor {
                        at: sort,
                        source,
                        key,
                    },
                    at,
                    machine,
                    what: self.what(source, key)?,
                })
            })
            .collect()
    }

    /// How many lines each group has in `day` (Unix seconds `from..until`),
    /// for `machine` when given, in [`ActivityGroup::ALL`]'s order.
    /// # Errors
    /// Reports database failures or a corrupt count.
    pub fn activity_counts(
        &self,
        machine: Option<&str>,
        day: (i64, i64),
    ) -> Result<Vec<(ActivityGroup, u64)>, Error> {
        let counted: Vec<(i64, i64)> = self
            .db
            .prepare(concat!(
                events!(),
                "SELECT grp,count(*) FROM events WHERE (?1 IS NULL OR machine=?1) AND at>=?2 AND at<?3 GROUP BY grp"
            ))?
            .query_map(params![machine, day.0, day.1], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<Result<_, _>>()?;
        ActivityGroup::ALL
            .into_iter()
            .map(|group| {
                let count = counted
                    .iter()
                    .find(|(number, _)| *number == group.number())
                    .map_or(0, |&(_, count)| count);
                u64::try_from(count)
                    .map(|count| (group, count))
                    .map_err(|_| Error::Corruption)
            })
            .collect()
    }

    /// What the line from `source` row `key` says happened.
    fn what(&self, source: i64, key: i64) -> Result<Activity, Error> {
        match source {
            0 => {
                let evidence: Option<String> = self.db.query_row(
                    "SELECT (SELECT evidence FROM interpretations i WHERE i.receipt=r.id ORDER BY version DESC LIMIT 1) FROM receipts r WHERE r.rowid=?1",
                    [key],
                    |r| r.get(0),
                )?;
                Ok(Activity::Stored(reading(evidence)?))
            }
            1 => {
                let evidence: String =
                    self.db
                        .query_row("SELECT evidence FROM parked WHERE id=?1", [key], |r| {
                            r.get(0)
                        })?;
                Ok(Activity::Parked {
                    item: key,
                    details: serde_json::from_str::<crate::parked::ParkingEvidence>(&evidence)?
                        .details,
                })
            }
            2 => {
                let action: Option<String> = self.db.query_row(
                    "SELECT json_extract(resolution,'$.action') FROM parked WHERE id=?1",
                    [key],
                    |r| r.get(0),
                )?;
                Ok(Activity::Resolved {
                    item: key,
                    action: action.unwrap_or_default(),
                })
            }
            3 => self.attempt(key),
            4 => self.order(key),
            5 => {
                let evidence: String = self.db.query_row(
                    "SELECT evidence FROM audit WHERE sequence=?1",
                    [key],
                    |r| r.get(0),
                )?;
                let audit: crate::Audit = serde_json::from_str(&evidence)?;
                Ok(Activity::Changed {
                    actor: audit.actor.as_str().to_owned(),
                    action: audit.action.as_str().to_owned(),
                    target: audit
                        .targets
                        .first()
                        .map(|target| target.as_str().to_owned()),
                })
            }
            _ => Err(Error::Corruption),
        }
    }

    fn attempt(&self, key: i64) -> Result<Activity, Error> {
        let (outcome, lab_app, order, evidence): (String, String, Option<String>, Option<String>) =
            self.db.query_row(
                concat!(
                    "SELECT a.outcome,o.destination,o.order_id,",
                    "(SELECT evidence FROM interpretations i WHERE i.receipt=o.receipt AND i.version=o.version) ",
                    "FROM attempts a JOIN outbox o ON o.id=a.delivery WHERE a.sequence=?1"
                ),
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        Ok(Activity::Attempt {
            lab_app,
            outcome: attempt_outcome(&outcome)?,
            carries: match order {
                Some(order) => Carries::OrderMessage { order },
                None => Carries::Results(reading(evidence)?),
            },
        })
    }

    fn order(&self, key: i64) -> Result<Activity, Error> {
        /// The fields of a stored order snapshot the feed shows.
        #[derive(Deserialize)]
        struct Snapshot {
            key: crate::MachineOrderKey,
            state: OrderState,
            #[serde(default)]
            items: Vec<serde::de::IgnoredAny>,
        }
        let evidence: String = self.db.query_row(
            "SELECT evidence FROM machine_order_history WHERE sequence=?1",
            [key],
            |r| r.get(0),
        )?;
        let snapshot: Snapshot = serde_json::from_str(&evidence)?;
        let sample: Option<String> = self
            .db
            .query_row(
                "SELECT sample FROM orders WHERE destination=?1 AND id=?2",
                params![
                    snapshot.key.destination.as_str(),
                    snapshot.key.order.as_str()
                ],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Activity::Order {
            order: snapshot.key.order.as_str().to_owned(),
            sample,
            // Orders reserved before items were recorded hold none.
            tests: (!snapshot.items.is_empty()).then_some(snapshot.items.len()),
            state: snapshot.state,
        })
    }
}

/// What a stored reading produced; none when there is no reading.
fn reading(evidence: Option<String>) -> Result<Option<RecentOutcome>, Error> {
    evidence
        .map(|evidence| serde_json::from_str(&evidence).map(|reading| RecentOutcome::of(&reading)))
        .transpose()
        .map_err(Error::from)
}

/// Reads an attempt's stored outcome, as `crate::Attempt` writes it.
fn attempt_outcome(stored: &str) -> Result<AttemptOutcome, Error> {
    let value: serde_json::Value = serde_json::from_str(stored)?;
    if let Some(word) = value.as_str() {
        return match word {
            "Done" => Ok(AttemptOutcome::Delivered),
            "uncertain (interrupted)" => Ok(AttemptOutcome::Interrupted),
            _ => Err(Error::Corruption),
        };
    }
    let object = value.as_object().ok_or(Error::Corruption)?;
    if let Some(at) = object.get("RetryAt") {
        return at
            .as_i64()
            .map(AttemptOutcome::RetryAt)
            .ok_or(Error::Corruption);
    }
    [
        ("Parked", AttemptOutcome::Rejected),
        ("Paused", AttemptOutcome::Paused),
        ("Conflict", AttemptOutcome::Conflict),
    ]
    .into_iter()
    .find_map(|(name, outcome)| object.contains_key(name).then_some(outcome))
    .ok_or(Error::Corruption)
}
